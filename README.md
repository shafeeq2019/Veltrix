# OpenDownload Manager

Kostenloser, quelloffener Download-Manager für **Windows 10/11** und **macOS** – mit
segmentierten Downloads, Fortsetzen nach Abbrüchen, persistenter Warteschlange und
Browser-Integration. Keine Werbung, kein Tracking, keine Registrierung.

Gebaut mit Tauri 2, Rust und React/TypeScript.

## Funktionen (Auszug)

* Bis zu 32 parallele Verbindungen pro Datei, wenn der Server Byte-Ranges unterstützt –
  mit dynamischer Neuaufteilung, damit alle Verbindungen bis zum Ende arbeiten
* Pause/Fortsetzen, auch nach App-Neustart oder Verbindungsabbruch; Erkennung, wenn sich die
  Datei auf dem Server geändert hat
* Wiederholungen mit Backoff, Beachtung von `Retry-After` (429/503), sichere Redirects
* Geschwindigkeitsmodi: Maximal, Ausgewogen, Benutzerdefiniert (KB/s, MB/s)
* Warteschlange mit Prioritäten, Parallelitätslimit und Zeitplan
* Browser-Erweiterung (Chrome, Edge, Brave, Vivaldi, Firefox): erkennt gestartete Downloads
  und fragt, ob die App übernehmen soll – oder übernimmt automatisch
* Dashboard, Kategorien, Suche, Sortierung, Mehrfachauswahl, Kontextmenüs, Tastaturkürzel,
  Drag-and-drop, Zwischenablage, helles/dunkles Design, Tray, Benachrichtigungen

Vollständige Liste und ehrliche Einschränkungen: [docs/FEATURES.md](docs/FEATURES.md).

## Projektstruktur

```
crates/odm-core/          Download-Engine, Segmente, Queue, Scheduler, SQLite, Einstellungen (Rust)
crates/odm-ipc/           Protokoll + lokaler, authentifizierter IPC-Transport
crates/odm-native-host/   Native-Messaging-Host für die Browser-Erweiterung
src-tauri/                Tauri-2-App: Commands, IPC-Endpunkt, Tray, Browser-Registrierung
src/                      React/TypeScript-Oberfläche (Tailwind, Radix)
extension/                WebExtension (MV3) für Chromium und Firefox
e2e/                      Ende-zu-Ende-Test Browser → Host → App → Datei
scripts/                  Build-Helfer (Sidecar)
docs/                     Architektur, Browser-Integration, Build, Sicherheit, Funktionen
```

## Schnellstart (Entwicklung)

Voraussetzungen: Rust (stable), Node.js 22, plattformspezifische Build-Tools
(siehe [docs/BUILD.md](docs/BUILD.md)).

```bash
npm ci
npm run app:dev                       # App mit Hot-Reload
cd extension && npm ci && npm run build   # Erweiterung → extension/dist/{chrome,firefox}
```

Erweiterung entpackt laden und Browser-Integration einrichten:
[docs/BROWSER_INTEGRATION.md](docs/BROWSER_INTEGRATION.md).

## Tests

```bash
cargo test --workspace          # Rust: Unit- + Integrationstests mit lokalem Testserver
npm run typecheck && npm test   # Frontend
cd extension && npm test        # Erfassungsregeln der Erweiterung
xvfb-run -a node e2e/browser-e2e.mjs   # Linux: kompletter Browser-Ablauf
```

## Installer bauen

```bash
npm run app:build    # Windows: NSIS + MSI, macOS: .app + DMG
```

Universal-Build für macOS, Signierung und Notarisierung: [docs/BUILD.md](docs/BUILD.md).
GitHub Actions bauen bei jedem Push Testpakete und bei Tags `v*` ein Draft-Release.

## Dokumentation

* [Architektur & Entscheidungen](docs/ARCHITECTURE.md)
* [Browser-Integration & Native Messaging](docs/BROWSER_INTEGRATION.md)
* [Build, Installation, Signierung](docs/BUILD.md)
* [Sicherheit & Datenschutz](docs/SECURITY.md)
* [Funktionen & Einschränkungen](docs/FEATURES.md)

## Lizenz

MIT – siehe [LICENSE](LICENSE).
