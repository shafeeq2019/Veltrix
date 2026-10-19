# Funktionsumfang und bekannte Einschränkungen

Stand: Version 0.1.0. ✅ = implementiert und automatisiert getestet, ☑️ = implementiert,
manuell bzw. nur per Build geprüft.

## Download-Engine

| Funktion | Status |
|---|---|
| HTTP/HTTPS, Streaming direkt auf die Festplatte | ✅ |
| Segmentierte Downloads (Range), konfigurierbar 1–32 Verbindungen, Mindestsegmentgröße | ✅ |
| Dynamisches Aufteilen laufender Segmente (Work-Stealing) | ✅ |
| Fallback auf Einzelverbindung ohne Range-Unterstützung | ✅ |
| Unbekannte Dateigröße | ✅ |
| Pause, Fortsetzen, Abbrechen (löscht Teildatei) | ✅ |
| Fortsetzen nach Abbruch und App-Neustart (If-Range, ETag/Last-Modified) | ✅ |
| Erkennung geänderter Ressourcen → sauberer Neustart | ✅ |
| Retry mit exponentiellem Backoff, `Retry-After` bei 429/503 | ✅ |
| Gekappte Range-Antworten werden fortgesetzt | ✅ |
| Redirects (max. 10), HTTPS→HTTP-Schutz | ✅ |
| Größen- und optionale SHA-256-Prüfung | ✅ |
| Globale Geschwindigkeitsbegrenzung, Modi Maximal/Ausgewogen/Benutzerdefiniert | ✅ |
| Globales Verbindungslimit | ✅ |
| Content-Disposition (RFC 6266/5987), sichere Dateinamen für Windows/macOS | ✅ |
| HTTP-Basic-Authentifizierung | ☑️ |
| Aktuelle, durchschnittliche Geschwindigkeit, ETA | ✅ |

## Warteschlange und Planung

| Funktion | Status |
|---|---|
| Persistente Warteschlange, Positionen, Prioritäten | ✅ |
| Maximale parallele Downloads (1 = nacheinander) | ✅ |
| Start/Stopp der Warteschlange, Start beim App-Start | ☑️ |
| Zeitplan (Start-/Stoppzeit, Wochentage) | ☑️ |
| Automatischer erneuter Versuch bei temporären Fehlern | ☑️ |
| Abgeschlossene verlassen die Warteschlange, bleiben im Verlauf (optional automatisch entfernen) | ✅ |
| Wiederherstellung nach Neustart, optional automatisches Fortsetzen | ✅ |

## Browser-Integration

| Funktion | Status |
|---|---|
| Erweiterung für Chromium-Browser und Firefox (MV3) | ✅ (Chromium E2E), ☑️ Firefox |
| Erkennung echter Downloads über die downloads-API, Pausieren bis zur Entscheidung | ✅ |
| Dialog „Jetzt / Warteschlange / Ignorieren / Immer für diesen Typ“ | ☑️ |
| Automatische Übernahme | ✅ |
| Dateitypliste, Ausschlüsse (Domain, Typ, URL), Mindestgröße, private Fenster | ✅ |
| Duplikaterkennung | ✅ |
| Kontextmenü für Links/Medien | ☑️ |
| Optionale Cookie-Weitergabe (explizit, nur im Speicher) | ☑️ |
| Native Messaging mit Token-Authentifizierung, App-Autostart | ✅ |
| Registrierung für Chrome, Edge, Brave, Vivaldi, Chromium, Firefox (Windows-Registry / macOS-Manifeste) | ☑️ |

## Handy-Ansicht (nur Anzeige)

| Funktion | Status |
|---|---|
| Statusseite für Smartphones im Heimnetz (Einstellungen → Handy, standardmäßig aus) | ✅ |
| Einrichtung per QR-Code, Zugriff nur mit 256-Bit-Schlüssel, Schlüssel erneuerbar | ✅ |
| Zeigt nur Dateiname, Status, Fortschritt, Geschwindigkeit, Restzeit – keine URLs, Ordner oder Steuerung | ✅ |

Einschränkungen: nur im selben Netzwerk (unterwegs per VPN wie Tailscale), Übertragung im
Heimnetz per HTTP ohne TLS – der Schlüssel könnte in einem fremden WLAN mitgelesen werden,
daher nur in vertrauenswürdigen Netzen aktivieren.

## Oberfläche

Dashboard, Liste mit Fortschritt/Geschwindigkeit/ETA/Status, Kategorien (Status, Typ,
benutzerdefiniert), Suche, Sortierung, Mehrfachauswahl (Strg/⌘, Shift), Kontextmenü,
Tastaturkürzel (Strg+N, Strg+V, Strg+F, Strg+A, Strg+,, Leertaste, Entf, Enter, Esc),
Drag-and-drop von Links, URL aus Zwischenablage, helles/dunkles Design, Tray/Menüleiste,
Minimieren in den Hintergrund, Benachrichtigungen, Bestätigung vor dem Ausführen von Programmen.

## Bekannte Einschränkungen

* **Mehr Verbindungen ≠ immer schneller.** Ohne Range-Unterstützung oder bei Servern, die
  pro Client drosseln, bringt Segmentierung nichts. Die App zeigt an, ob ein Download
  fortsetzbar ist.
* **Firefox** kennt kein `onDeterminingFilename`; der Dateiname stammt aus `onCreated` und
  kann bei manchen Servern erst später feststehen. Dauerhafte Installation erfordert eine
  Signierung durch Mozilla.
* **Store-Versionen** der Erweiterung haben andere IDs, die in der App eingetragen werden müssen.
* **Downloads, die nur im Browser existieren** (`blob:`, `data:`, per JavaScript erzeugte
  Dateien, POST-Formulare) können nicht übergeben werden und bleiben im Browser.
* **Sitzungsgebundene Links:** Ohne aktivierte Cookie-Weitergabe kann der Server die App
  ablehnen (403). Cookies, Passwörter und Basic-Auth gehen bei einem App-Neustart verloren;
  solche Downloads müssen neu gestartet bzw. die Anmeldedaten neu eingegeben werden.
  Abgelaufene signierte URLs lassen sich nicht fortsetzen.
* **„Probe“ im Dialog „Neuer Download“** stellt eine Anfrage an den Server. Bei Links, die
  nur einmal gültig sind, besser über die Browser-Erweiterung übernehmen (dort entfällt die Probe).
* **Ausgewogener Modus** ist eine Heuristik (80 % der beobachteten Spitzenrate), keine echte
  Messung der Gesamtauslastung des Netzwerks durch andere Programme.
* **Keine Protokolle außer HTTP(S)** (kein FTP, BitTorrent, HLS/DASH).
* **Oberfläche nur auf Deutsch**; die Texte sind zentral in den Komponenten und für eine
  spätere Übersetzung vorbereitet (`language` in den Einstellungen).
* **Signierung/Notarisierung** benötigt eigene Zertifikate (siehe BUILD.md); unsignierte
  Builds lösen SmartScreen-/Gatekeeper-Warnungen aus.
* Die Windows- und macOS-Bundles werden in CI gebaut; Tray, Benachrichtigungen und
  Registrierung wurden in dieser Version nur unter Linux interaktiv geprüft.
* MSI-Deinstallation entfernt die Registry-Einträge nicht automatisch (NSIS schon).
