# Browser-Integration

Die Integration besteht aus drei Teilen:

1. **Erweiterung** (`extension/`) für Chromium-Browser und Firefox (Manifest V3).
2. **Native-Messaging-Host** `veltrix-native-host` (wird mit der App installiert).
3. **IPC-Endpunkt** in der laufenden Desktop-App.

## Was die Erweiterung tut – und was nicht

* Sie nutzt ausschließlich die offizielle `downloads`-API. Diese meldet nur echte
  Datei-Downloads; Seitennavigation, Bilder, Skripte, Stylesheets, API-Aufrufe und
  Streaming-Medien erscheinen dort nicht und werden daher nie abgefangen.
* Für jeden Download wird lokal geprüft (`extension/src/rules.ts`):
  Dateityp-Liste, „immer übernehmen“, Ausschlüsse (Domain, Dateityp, URL-Bestandteil),
  Mindestgröße, private Fenster. `blob:`, `data:` und `file:` bleiben im Browser.
* Passt ein Download, wird er im Browser **pausiert** und der App angeboten.
  * „Jetzt herunterladen“ / „Zur Warteschlange“ ⇒ Browser-Download wird abgebrochen und aus
    der Liste entfernt.
  * „Ignorieren“, App-Fehler oder 120 s ohne Antwort ⇒ Browser-Download läuft weiter.
* Übermittelt werden nur: URL (und End-URL nach Redirects), Dateiname, MIME-Typ, Größe,
  optional Referrer, optional Cookies. Kein Verlauf, keine Seiteninhalte.
* **Cookies** werden nur gesendet, wenn du im Popup „Cookies mitsenden“ aktivierst – dabei
  fragt der Browser nach der Berechtigung. Die App hält sie nur im Arbeitsspeicher und nur
  für diesen einen Download.
* Kontextmenü „Mit Veltrix herunterladen“ für Links, Video- und Audio-Elemente.

Die Einstellungen (Dateitypen, Ausschlüsse, automatische Übernahme) liegen in der App
(Einstellungen → Browser); die Erweiterung lädt sie alle 5 Minuten neu.

## Installation

### 1. App installieren

Der Installer (Windows) bzw. das App-Bundle (macOS) enthält `veltrix-native-host`. Beim ersten Start
registriert die App den Host für alle erkannten Browser des aktuellen Benutzers
(ohne Administratorrechte). Status und manuelle Registrierung: **Einstellungen → Browser**.

### 2. Erweiterung laden

Bis zur Veröffentlichung in den Stores wird die Erweiterung entpackt geladen:

```bash
cd extension
npm ci
npm run build          # erzeugt dist/chrome und dist/firefox
```

* **Chrome / Edge / Brave / Vivaldi:** `chrome://extensions` → Entwicklermodus →
  „Entpackte Erweiterung laden“ → `extension/dist/chrome`.
  Die ID ist durch den `key` im Manifest fest: `pjmfnhocabgijmcbdcbkfeaogjnhffea`.
* **Firefox (≥ 128):** `about:debugging#/runtime/this-firefox` → „Temporäres Add-on laden“ →
  `extension/dist/firefox/manifest.json`. Add-on-ID: `veltrix@veltrix.local`.
  Für dauerhafte Installation muss das Add-on über addons.mozilla.org signiert werden.

Wird die Chromium-Erweiterung im Chrome Web Store veröffentlicht, erhält sie eine andere ID.
Diese muss unter **Einstellungen → Browser → Zusätzliche Chromium-Erweiterungs-IDs**
eingetragen werden (die App aktualisiert dann die Manifeste).

## Registrierung im Detail

Hostname: `app.veltrix`

### Windows

Die App schreibt zwei Manifeste nach `%APPDATA%\app.veltrix\native-messaging\`
(`chromium.json`, `firefox.json`) und setzt den Standardwert dieser Registry-Schlüssel auf den
jeweiligen Pfad:

```
HKCU\Software\Google\Chrome\NativeMessagingHosts\app.veltrix      (Chrome, Brave)
HKCU\Software\Chromium\NativeMessagingHosts\app.veltrix
HKCU\Software\Microsoft\Edge\NativeMessagingHosts\app.veltrix
HKCU\Software\Mozilla\NativeMessagingHosts\app.veltrix
```

Der NSIS-Deinstaller entfernt diese Schlüssel (`src-tauri/windows/installer-hooks.nsh`).
Beim MSI-Paket bitte vor der Deinstallation **Einstellungen → Browser → Entfernen** wählen;
verbleibende Schlüssel zeigen sonst auf eine nicht mehr vorhandene Datei und sind wirkungslos.

### macOS

Manifest-Dateien `app.veltrix.json` werden nur für installierte Browser angelegt:

```
~/Library/Application Support/Google/Chrome/NativeMessagingHosts/
~/Library/Application Support/Chromium/NativeMessagingHosts/
~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/
~/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/
~/Library/Application Support/Vivaldi/NativeMessagingHosts/
~/Library/Application Support/Mozilla/NativeMessagingHosts/
```

`path` zeigt auf `/Applications/Veltrix.app/Contents/MacOS/veltrix-native-host`.
Wird die App verschoben, bei Bedarf in den Einstellungen erneut „Registrieren“ wählen
(passiert auch automatisch bei jedem App-Start).

### Manuell

```bash
veltrix-native-host --manifest chrome pjmfnhocabgijmcbdcbkfeaogjnhffea > app.veltrix.json
veltrix-native-host --manifest firefox > app.veltrix.json
```

## Verhalten, wenn die App nicht läuft

* `get_config`/`ping`: Der Host meldet `app_not_running`; die Erweiterung nutzt die zuletzt
  bekannte Konfiguration.
* Ein konkreter Download (`offer`): Der Host startet die App im Hintergrund
  (`--background`) und wartet bis zu 20 s auf den IPC-Endpunkt. Klappt das nicht, läuft
  der Download im Browser weiter und die Erweiterung zeigt ein „!“-Badge mit Hinweis im Popup.
* Ist der Host gar nicht registriert, fängt die Erweiterung nichts ab und prüft alle
  5 Minuten erneut.

## Protokoll

Native Messaging: 32-Bit-Länge (native Byte-Reihenfolge) + UTF-8-JSON, max. 1 MiB.
Gleiches Framing zwischen Host und App.

Anfragen (Erweiterung → Host):

```json
{ "v": 1, "id": "a1b2", "type": "ping" }
{ "v": 1, "id": "a1b3", "type": "get_config" }
{ "v": 1, "id": "a1b4", "type": "offer",
  "url": "https://example.org/file.zip", "final_url": null, "filename": "file.zip",
  "mime": "application/zip", "size": 123456, "referrer": "https://example.org/",
  "cookies": null, "source": "browser_download", "private": false }
```

Antworten:

```json
{ "v": 1, "id": "a1b2", "type": "pong", "app_version": "0.1.0" }
{ "v": 1, "id": "a1b3", "type": "config", "config": { "enabled": true, "file_types": ["zip"] } }
{ "v": 1, "id": "a1b4", "type": "decision", "decision": "accepted" }   // accepted | queued | ignored | duplicate
{ "v": 1, "id": "a1b4", "type": "error", "code": "bad_request", "message": "url must use http or https" }
```

Host → App: `{ "token": "<64 hex>", "origin": "chrome-extension://…/", "request": { … } }`.

Validierung (Host **und** App): Protokollversion, ID-Format, nur `http`/`https`-URLs mit Host,
Längenlimits (URL 16 KiB, Name 1 KiB, Cookies 16 KiB), keine Steuerzeichen, unbekannte
Nachrichtentypen werden abgelehnt.
