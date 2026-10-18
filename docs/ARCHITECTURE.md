# Architektur

Dieses Dokument beschreibt Anforderungen (Phase 1), Module, Schnittstellen und
die wichtigsten Entscheidungen von OpenDownload Manager.

## 1. Präzisierte Anforderungen

| Bereich | Entscheidung |
|---|---|
| Plattformen | Windows 10/11 (x64), macOS 10.15+ (Intel und Apple Silicon, Universal-Binary). Linux wird für Entwicklung und CI gebaut, ist aber kein offizielles Ziel. |
| Protokolle | HTTP/1.1 und HTTPS. Kein FTP, kein BitTorrent, keine Streaming-Protokolle (HLS/DASH). |
| Browser | Chromium-basierte Browser (Chrome, Edge, Brave, Vivaldi, Chromium) und Firefox ≥ 128 über eine WebExtension (Manifest V3) und Native Messaging. |
| Datenhaltung | Lokal: SQLite in `app_data_dir`. Keine Cloud, keine Konten, keine Telemetrie. |
| Sicherheit | TLS-Prüfung über den Zertifikatsspeicher des Betriebssystems, nicht abschaltbar. Keine Netzwerkdienste, die von außen erreichbar sind. |
| Nicht-Ziele | Umgehung von DRM, Paywalls, Zugriffskontrollen; Video-Extraktion von Streaming-Seiten; heimliche Überwachung des Browsers. |

## 2. Überblick

```
┌──────────────────────┐   Native Messaging (stdin/stdout,     ┌─────────────────────────┐
│  Browser-Erweiterung │   längenpräfixiertes JSON)            │  odm-native-host        │
│  (MV3, TypeScript)   │ ─────────────────────────────────────▶│  (Rust, kurzlebig)      │
│  downloads-API,      │ ◀───────────────────────────────────── │  validiert, leitet      │
│  contextMenus        │                                        │  weiter, startet App    │
└──────────────────────┘                                        └────────────┬────────────┘
                                                                             │ Unix-Socket (0600, UID-Prüfung)
                                                                             │ bzw. Named Pipe (pro Benutzer)
                                                                             │ + 256-Bit-Token
┌────────────────────────────────────────────────────────────────────────────▼────────────┐
│ Desktop-App (Tauri 2, src-tauri)                                                        │
│  ┌───────────────┐  ┌──────────────────┐  ┌───────────────┐  ┌───────────────────────┐   │
│  │ IPC-Server    │  │ Capture / Offers │  │ Tauri-Commands│  │ Tray, Notifications,  │   │
│  │ (odm-ipc)     │─▶│ (Dialog/Auto)    │  │ (typisiert)   │  │ Native-Host-Registr.  │   │
│  └───────────────┘  └────────┬─────────┘  └───────┬───────┘  └───────────────────────┘   │
│                              ▼                    ▼                                     │
│  ┌──────────────────────────────── odm-core ─────────────────────────────────────────┐   │
│  │ Manager (Queue, Scheduler, Events) ─ Engine (Segmente, Work-Stealing, Retry)      │   │
│  │ HTTP-Client (reqwest/rustls) ─ RateLimiter ─ Store (SQLite) ─ Settings ─ Filename │   │
│  └───────────────────────────────────────────────────────────────────────────────────┘   │
│         ▲ Events ("engine-event")                              Commands (invoke) ▲       │
└─────────┼────────────────────────────────────────────────────────────────────────┼───────┘
          │                                                                        │
┌─────────┴────────────────────────────────────────────────────────────────────────┴───────┐
│ React-UI (TypeScript, Tailwind, Radix): Dashboard, Liste, Dialoge, Einstellungen         │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

## 3. Module

| Anforderung | Umsetzung |
|---|---|
| Core Download Engine | `crates/odm-core/src/engine.rs` – Lauf eines Downloads von der ersten Anfrage bis zur atomaren Umbenennung. |
| HTTP Client | `crates/odm-core/src/http.rs` – Client-Konfiguration, Range-/If-Range-Anfragen, Content-Range/Retry-After-Parser, `probe`. |
| Segment Manager | `engine.rs` (`plan_segments`, `Shared::claim`, Worker) – Planung, dynamisches Aufteilen (Work-Stealing), Schreiben an Positionen. |
| Queue Manager | `crates/odm-core/src/manager.rs` – Warteschlange, Positionen, Prioritäten, Parallelitätslimit. |
| Download Scheduler | `manager.rs` (`schedule_tick`, `apply_schedule`) – Zeitplan, automatische Wiederholung, Start wartender Downloads. |
| Browser Integration | `extension/` (WebExtension) und `src-tauri/src/capture.rs` (Angebote, Dialog, „immer übernehmen“). |
| IPC Layer | `crates/odm-ipc` (Protokoll, Framing, Transport, Server) und `crates/odm-native-host`. |
| Persistent Storage | `crates/odm-core/src/storage.rs` – SQLite (WAL), Schema-Version per `user_version`. |
| Application Settings | `crates/odm-core/src/settings.rs` – serde-Defaults, `normalize()` begrenzt alle Werte. |
| Notification Service | `src-tauri/src/lib.rs` (`forward_events`) über `tauri-plugin-notification`. |
| Desktop UI | `src/` – React 19, Zustand-Store, Radix-Dialoge/-Menüs, Tailwind 4. |
| Logging und Diagnostics | `tracing` mit Dateiausgabe `app_data_dir/logs/odm.log` (Rotation bei 5 MiB). URLs werden mit `redact_url` ohne Query und Zugangsdaten geloggt. |

## 4. Download-Engine im Detail

1. **Probe = erstes Segment.** Die erste Anfrage ist `GET` mit `Range: bytes=0-`.
   * `206` + `Content-Range: bytes 0-…/TOTAL` ⇒ Range-fähig, Größe bekannt. Die Antwort
     wird direkt als Segment 0 weiterverwendet (kein zusätzlicher Round-Trip, einmalig
     gültige Links funktionieren weiter).
   * `200` ⇒ keine Ranges: Einzelverbindung mit derselben Antwort.
   * `416` (leere Datei) ⇒ Wiederholung ohne Range.
2. **Planung.** `segments = clamp(size / min_segment_size, 1, max)`; Standard 8 Verbindungen,
   mindestens 1 MiB pro Segment. Im Modus „Ausgewogen“ maximal 4.
3. **Work-Stealing.** Ist ein Worker fertig, halbiert er den größten noch offenen Bereich
   eines anderen Segments (nur wenn ≥ 2 × Mindestgröße). So bleiben alle Verbindungen bis zum
   Ende ausgelastet, ohne zusätzliche Verbindungen zu öffnen.
4. **Schreiben.** Chunks werden in einem Puffer (≤ 512 KiB) gesammelt und per positionalem
   Schreiben (`pwrite` / `seek_write`) in `datei.ext.odmpart` geschrieben. Die Datei wird bei
   bekannter Größe vorab auf volle Länge gesetzt. Kein vollständiges Laden in den RAM.
5. **Checkpoints.** Alle 2 s: Fortschrittswerte lesen → `sync_data` → Segmentstand in SQLite.
   Der gespeicherte Stand ist daher nie größer als die tatsächlich auf Disk liegenden Daten.
6. **Validierung.** Folgeanfragen senden `If-Range` (starker ETag oder Last-Modified).
   Antwortet der Server mit `200`, abweichendem `Content-Range`-Start, anderer Gesamtgröße
   oder anderem ETag ⇒ `ResourceChanged` ⇒ ein automatischer Neustart von Byte 0.
7. **Fehler.** Temporär (Timeout, Verbindungsabbruch, 5xx, 408, 429) ⇒ Retry mit
   exponentiellem Backoff (1 s … 60 s, ±20 % Jitter) bzw. `Retry-After` (max. 10 min).
   Der Zähler wird zurückgesetzt, sobald wieder Daten fließen. Dauerhaft (401/403/404/410,
   TLS, Disk voll, Prüfsumme) ⇒ sofort `Failed` mit Fehlercode.
8. **Abschluss.** Größe prüfen, optional SHA-256 prüfen, `sync_all`, Handle schließen,
   atomares `rename` auf den finalen Namen (bei Konflikt `name (1).ext`).

Server, die Range-Antworten kürzen (z. B. max. 1 MiB pro Antwort), werden erkannt und ohne
Fehlerzählung fortgesetzt.

### Geschwindigkeitsmodi

| Modus | Verhalten |
|---|---|
| Maximale Geschwindigkeit (Standard) | Keine Drosselung. Verbindungen pro Download und global nach Einstellungen (8 / 32). |
| Ausgewogen | ≤ 2 parallele Queue-Downloads, ≤ 4 Segmente, ≤ 8 Verbindungen gesamt und Soft-Limit von 80 % der in dieser Sitzung beobachteten Spitzenbandbreite. |
| Benutzerdefiniert | Globaler Token-Bucket (KB/s oder MB/s), fair über alle Verbindungen verteilt. |

Mehrere Verbindungen beschleunigen nur, wenn der Server Ranges unterstützt und pro Verbindung
drosselt. HTTP/2 ist bewusst deaktiviert, damit Segmente tatsächlich eigene TCP-Verbindungen
nutzen.

## 5. Schnittstellen

### UI ↔ Backend (Tauri)

* Commands (`src-tauri/src/commands.rs`, Wrapper in `src/lib/api.ts`):
  `list_downloads`, `add_download`, `probe_url`, `pause_downloads`, `resume_downloads`,
  `cancel_downloads`, `retry_downloads`, `remove_downloads`, `set_priority`, `set_in_queue`,
  `move_in_queue`, `set_credentials`, `start_queue`, `stop_queue`, `get_settings`,
  `update_settings`, `open_file`, `show_in_folder`, `resolve_offer`, `pending_offers`,
  `integration_status`, `register_browser_integration`, `unregister_browser_integration`.
* Fehler: `{ code: ErrorCode, message }` – `ErrorCode` ist ein stabiles Enum (snake_case).
* Events: `engine-event` (`added`, `updated`, `removed`, `progress` alle 500 ms,
  `completed`, `failed`, `queue_state`), `capture-offer`, `capture-offer-expired`.

### Erweiterung ↔ Host ↔ App

Siehe [BROWSER_INTEGRATION.md](BROWSER_INTEGRATION.md#protokoll).

## 6. Datenmodell (SQLite)

Tabelle `downloads`: `id`, `url`, `final_url`, `referrer`, `file_name`, `name_locked`,
`save_dir`, `total_size`, `downloaded`, `status`, `error_code`, `error_message`,
`error_http_status`, `error_retryable`, `category`, `priority`, `in_queue`, `queue_position`,
`resumable`, `etag`, `last_modified`, `mime`, `segments` (JSON: start/end/written),
`connections`, `expected_sha256`, `retry_count`, `created_at`, `updated_at`, `completed_at`,
`avg_speed`, `active_ms`, `requires_auth`.

Tabelle `settings`: Schlüssel/Wert (`app` = Einstellungen als JSON, `resume_ids`,
`bandwidth_estimate`).

Zugangsdaten (HTTP-Auth, Cookies) werden **nicht** gespeichert; `requires_auth` markiert
Downloads, die nach einem Neustart erneut Anmeldedaten brauchen.

### Wiederherstellung

Beim Start: aktive Downloads → `paused` (oder Neustart bei „nach Neustart fortsetzen“),
fehlende Teildateien ⇒ Neustart bei 0, nicht fortsetzbare Teildaten werden verworfen.
Beim Beenden pausiert die App alle Downloads (max. 5 s) und persistiert ihren Stand.

## 7. Bibliotheken und Plattformkompatibilität

| Bibliothek | Version | Windows | macOS | Anmerkung |
|---|---|---|---|---|
| tauri | 2.12 | WebView2 | WKWebView | Tray über `tray-icon`-Feature |
| reqwest | 0.13 (rustls, aws-lc-rs, platform-verifier) | ✓ | ✓ | OS-Zertifikatsspeicher; aws-lc-sys benötigt auf Windows den MSVC-Toolchain (in CI vorhanden) |
| rusqlite | 0.40 (`bundled`) | ✓ | ✓ | SQLite wird mitkompiliert, keine Systemabhängigkeit |
| tokio | 1.x | Named Pipes | Unix-Sockets | IPC-Server |
| winreg | 0.56 | ✓ | – | nur Windows (Native-Messaging-Registrierung) |
| tauri-plugin-{dialog,notification,opener,clipboard-manager,single-instance} | 2.x | ✓ | ✓ | |
| React 19, Vite 8, Tailwind 4, Radix | aktuell | – | – | Frontend, plattformneutral |

Alternativen, falls nötig: `native-tls` statt rustls (Feature-Wechsel in `odm-core/Cargo.toml`),
`interprocess` statt eigener Transport-Implementierung.

## 8. Wichtige Entscheidungen

* **Native Messaging statt lokalem HTTP-Server:** Browser erzwingen `allowed_origins`;
  keine offenen Ports, kein CSRF-Risiko durch Webseiten.
* **Eigener Rust-Kern ohne Tauri-Abhängigkeit:** testbar mit lokalem Testserver, wiederverwendbar.
* **Ein Datensatz, zwei Zähler:** `received` (Anzeige) und `written` (persistiert) trennen
  Geschwindigkeitsanzeige von Wiederaufnahme-Sicherheit.
* **Dialog in der App, nicht im Browser:** Der Browser-Download wird pausiert, bis die
  Entscheidung vorliegt; bei „Ignorieren“, Fehler oder Timeout läuft er im Browser weiter.
