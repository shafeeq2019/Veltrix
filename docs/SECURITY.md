# Sicherheit und Datenschutz

## Grundsätze

* Keine Werbung, kein Tracking, keine Telemetrie, keine Registrierung, keine Cloud-Dienste.
  Die App öffnet ausschließlich Verbindungen zu den Download-URLs, die der Benutzer angibt
  oder im Browser bestätigt.
* Heruntergeladene Dateien werden **nur gespeichert**. Geöffnet werden sie nur auf
  ausdrückliche Aktion (Doppelklick / „Datei öffnen“). Für ausführbare Typen
  (`exe`, `msi`, `dmg`, `pkg`, `app`, `bat`, `ps1`, `sh`, `jar`, …) ist zusätzlich eine
  Bestätigung erforderlich, die erklärt, dass die Datei ausgeführt wird. Das Backend erzwingt
  das unabhängig von der UI (`ErrorCode::ConfirmationRequired`).

## Bedrohungsmodell und Maßnahmen

| Bedrohung | Maßnahme |
|---|---|
| Webseite löst über die App Downloads aus | Kein lokaler HTTP-Server. Nur die Erweiterung mit fester ID darf den Host starten (`allowed_origins` / `allowed_extensions`, vom Browser erzwungen). Die Erweiterung reagiert nur auf Downloads, die der Browser selbst gestartet hat. |
| Anderer lokaler Prozess/Benutzer spricht die App an | Unix-Socket im Benutzerverzeichnis (Ordner 0700, Socket 0600) mit Prüfung der Peer-UID; unter Windows Named Pipe pro Benutzer mit `reject_remote_clients` und `first_pipe_instance` (verhindert Squatting). Zusätzlich 256-Bit-Zufallstoken pro App-Start in einer nur für den Benutzer lesbaren Datei, konstantzeitiger Vergleich. |
| Manipulierte Nachrichten | Typisiertes JSON (unbekannte Typen werden abgelehnt), Versionsprüfung, Größenlimit 1 MiB, Längenlimits, keine Steuerzeichen, nur `http`/`https` – geprüft im Host **und** in der App, URL-Validierung erneut im Kern. Erste Nachricht muss innerhalb von 10 s eintreffen. |
| Path Traversal / ungültige Namen | Jeder Name aus Server, Browser oder UI läuft durch `sanitize_file_name`: nur letzte Pfadkomponente, verbotene Zeichen ersetzt, reservierte Windows-Namen (`CON`, `NUL`, `COM1` …) entschärft, abschließende Punkte/Leerzeichen entfernt, Unicode-Bidi-Steuerzeichen (Extension-Spoofing) ersetzt, Länge ≤ 200 Bytes. Zielordner müssen absolut sein und dürfen kein `..` enthalten. |
| TLS-Angriffe | rustls mit dem Zertifikatsspeicher des Betriebssystems; es gibt keine Option zum Abschalten. HTTPS→HTTP-Weiterleitungen werden standardmäßig abgelehnt. |
| Weitergabe von Zugangsdaten an fremde Hosts | reqwest entfernt `Authorization` und `Cookie` bei Redirects auf einen anderen Host. Zugangsdaten aus `https://user:pass@…` werden aus der URL entfernt und nie gespeichert. |
| Offenlegung sensibler Daten in Logs | Logs enthalten URLs nur als `schema://host/pfad?<redacted>`; Fehlermeldungen von reqwest werden ohne URL übernommen. Cookies und Passwörter werden nie geloggt. |
| Ressourcenerschöpfung | Globales Verbindungslimit (Semaphore, Standard 32, max. 128), max. 32 Segmente pro Download, begrenzte Wiederholungen mit Backoff, Puffer ≤ 512 KiB pro Verbindung, Einstellungen werden beim Laden auf sichere Bereiche begrenzt. |
| Beschädigte Dateien | Schreiben in `.odmpart`, persistierter Fortschritt nur nach `fsync`, Validierung über ETag/Last-Modified/Content-Range, Größenprüfung, optional SHA-256, atomares Umbenennen. |

## Handy-Ansicht

Standardmäßig aus. Eingeschaltet lauscht die App auf einem Port im lokalen Netz und liefert
ausschließlich eine Seite und `GET /api/state` (nur lesend). Jede Abfrage braucht den
64-stelligen Schlüssel aus dem QR-Code (konstantzeitiger Vergleich); er steht im URL-Fragment
und wird daher nicht an Server oder Logs übertragen. Die Antwort enthält keine URLs, Pfade
oder Zugangsdaten. Strikte CSP, `no-referrer`, kein Framing. Da im Heimnetz kein TLS
verwendet wird, die Funktion nur in vertrauenswürdigen Netzen nutzen; „Neuen Schlüssel
erstellen“ sperrt alle bisher verbundenen Geräte aus.

## Zugangsdaten

* HTTP-Basic-Anmeldedaten und Cookies werden ausschließlich im Arbeitsspeicher gehalten,
  nur für den betroffenen Download verwendet und nach einem App-Neustart verworfen. Downloads,
  die sie benötigten, sind mit `requires_auth` markiert.
* Cookies übermittelt die Erweiterung nur nach ausdrücklicher Aktivierung im Popup und
  erteilter Browser-Berechtigung.
* Die Software umgeht weder DRM noch Paywalls oder Zugriffskontrollen.

## Sicherheitslücken melden

Bitte nicht öffentlich als Issue, sondern über „Security → Report a vulnerability“ im
GitHub-Repository.
