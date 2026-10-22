# Entwicklung, Build und Veröffentlichung

## Voraussetzungen

| | Windows 10/11 | macOS 10.15+ |
|---|---|---|
| Rust | stable ≥ 1.80 (`rustup`), Toolchain `x86_64-pc-windows-msvc` | stable ≥ 1.80 (`rustup`) |
| C/C++ | Visual Studio 2022 Build Tools („Desktopentwicklung mit C++“) | Xcode Command Line Tools (`xcode-select --install`) |
| WebView | WebView2 (in Windows 11 enthalten; der Installer lädt ihn bei Bedarf) | WKWebView (systemseitig) |
| Node.js | 22 LTS | 22 LTS |

Linux (nur Entwicklung/CI): `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev`.

Es werden **keine** Umgebungsvariablen benötigt (daher keine `.env.example`). Optional:

* `ODM_DATA_DIR` – anderes Datenverzeichnis (Tests, portable Nutzung)
* `ODM_LOG` – Log-Filter, z. B. `debug` oder `odm_core=trace`

## Entwicklung

```bash
npm ci
npm run app:dev          # baut veltrix-native-host (debug) und startet `tauri dev`
```

`tauri dev` startet Vite (Port 1420) und die App mit Hot-Reload. Im Debug-Build liegt
`veltrix-native-host` in `target/debug/` neben der App; die App registriert ihn beim Start für
installierte Browser.

Nur das UI im Browser ansehen: `npm run dev` (ohne Backend-Daten, Hinweisbanner wird angezeigt).

## Tests

```bash
cargo test --workspace                 # Engine, Storage, IPC, Native Host (inkl. lokalem Testserver)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run typecheck && npm test          # Frontend
cd extension && npm run typecheck && npm test && npm run build
xvfb-run -a node e2e/browser-e2e.mjs   # Linux: Chromium + Erweiterung + Host + App, Ende-zu-Ende
```

Die Rust-Integrationstests (`crates/odm-core/tests/engine_integration.rs`) starten einen
lokalen axum-Server, der Range-Unterstützung, fehlende Ranges, unbekannte Größe,
Verbindungsabbrüche, gekappte Range-Antworten, 429/503, Redirects, sich ändernde Dateien,
Content-Disposition und langsame Übertragung simuliert.

## Release-Build lokal

```bash
npm run app:build
```

Das entspricht:

```bash
node scripts/build-sidecar.mjs                     # veltrix-native-host → src-tauri/binaries/veltrix-native-host-<triple>
npx tauri build --config src-tauri/tauri.bundle.conf.json
```

`tauri.bundle.conf.json` fügt den Host als `externalBin` hinzu. Er wird neben die Haupt-
anwendung installiert (`…\Veltrix\veltrix-native-host.exe` bzw.
`…app/Contents/MacOS/veltrix-native-host`).

Ergebnisse unter `target/release/bundle/`:

* Windows: `nsis/*-setup.exe` (Installation pro Benutzer, ohne Adminrechte), `msi/*.msi`
* macOS: `macos/Veltrix.app`, `dmg/*.dmg`

### macOS Universal (Intel + Apple Silicon)

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
node scripts/build-sidecar.mjs --target universal-apple-darwin
npx tauri build --target universal-apple-darwin --config src-tauri/tauri.bundle.conf.json
```

## GitHub Actions

* `.github/workflows/ci.yml` – bei jedem Push/PR: Rust (fmt, clippy, Tests) auf Ubuntu,
  Windows und macOS; Frontend; Erweiterung; Browser-E2E (Linux, xvfb);
  Bundle-Check (echte NSIS/MSI- und DMG-Builds als Artefakte).
* `.github/workflows/release.yml` – bei Tag `v*`: Windows-x64- und macOS-Universal-Installer
  über `tauri-action` als Draft-Release, Erweiterungs-ZIPs als Artefakt.

## Unsignierte Testbuilds auf dem Mac öffnen

Die CI-Artefakte sind Universal-Builds (Intel + Apple Silicon) und nur ad-hoc signiert.
Gatekeeper blockiert sie daher beim ersten Start. Entweder im Finder Rechtsklick → „Öffnen“
(bzw. Systemeinstellungen → Datenschutz & Sicherheit → „Dennoch öffnen“) oder:

```bash
xattr -dr com.apple.quarantine "/Applications/Veltrix.app"
```

Die Meldung „… wird auf diesem Mac nicht unterstützt“ bedeutet dagegen eine falsche
Prozessorarchitektur (z. B. ein reiner Apple-Silicon-Build auf einem Intel-Mac).

## Signierung und Notarisierung

Ohne Signatur funktionieren die Builds, aber Windows SmartScreen bzw. macOS Gatekeeper warnen.

### macOS

1. Apple-Developer-Programm, Zertifikat „Developer ID Application“ als `.p12` exportieren.
2. Repository-Secrets setzen:
   * `APPLE_CERTIFICATE` – Base64 des `.p12` (`base64 -i cert.p12 | pbcopy`)
   * `APPLE_CERTIFICATE_PASSWORD`
   * `APPLE_SIGNING_IDENTITY` – z. B. `Developer ID Application: Name (TEAMID)`
   * `APPLE_ID`, `APPLE_PASSWORD` (app-spezifisches Passwort), `APPLE_TEAM_ID` – für die Notarisierung
3. `tauri-action` signiert App und Sidecar (`veltrix-native-host`) mit Hardened Runtime und
   notarisiert das DMG automatisch.

Lokal: dieselben Variablen exportieren und `npm run app:build` ausführen.
Prüfen: `spctl -a -vv "Veltrix.app"` und `xcrun stapler validate *.dmg`.

### Windows

Code-Signing-Zertifikat (OV/EV) besorgen und in `src-tauri/tauri.conf.json` unter
`bundle.windows` `certificateThumbprint`, `digestAlgorithm: "sha256"` und `timestampUrl`
eintragen, oder `bundle.windows.signCommand` für Cloud-HSMs (z. B. Azure Trusted Signing)
verwenden. Der Sidecar wird von Tauri mitsigniert.

## Deinstallation

* **Windows:** Über „Apps & Features“. Der NSIS-Deinstaller entfernt die Native-Messaging-
  Registry-Schlüssel. Daten bleiben in `%APPDATA%\app.veltrix` (vom Deinstaller
  optional löschbar).
* **macOS:** In der App **Einstellungen → Browser → Entfernen**, dann App in den Papierkorb.
  Daten: `~/Library/Application Support/app.veltrix`.
