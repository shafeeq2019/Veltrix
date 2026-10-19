import { RefreshCw, Smartphone } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage } from "../lib/api";
import { useStore } from "../lib/store";
import type { RemoteInfo } from "../lib/types";
import { Button, Switch } from "./ui";

/** Settings tab for the read-only phone status page. Changes apply immediately. */
export function RemoteViewPanel() {
  const settings = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);
  const toast = useStore((s) => s.toast);
  const [info, setInfo] = useState<RemoteInfo | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    api.remoteViewInfo().then(setInfo).catch(() => setInfo(null));
  }, []);
  useEffect(refresh, [refresh, settings?.remote_view_enabled, settings?.remote_view_token]);

  if (!settings) return null;

  const toggle = async (enabled: boolean) => {
    setBusy(true);
    try {
      setSettings(await api.updateSettings({ ...settings, remote_view_enabled: enabled }));
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const resetToken = async () => {
    try {
      setSettings(await api.remoteViewResetToken());
      toast("success", "Neuer Zugangsschlüssel erstellt – bitte den QR-Code neu scannen.");
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-start gap-3 rounded-lg border border-line bg-panel-2 p-3 text-xs text-muted">
        <Smartphone size={18} className="mt-0.5 shrink-0 text-accent" />
        <p>
          Zeigt den Fortschritt deiner Downloads auf dem Handy – nur ansehen, keine Steuerung. Das Handy muss im selben WLAN sein. Angezeigt werden nur Dateinamen,
          Status und Fortschritt, keine Links oder Ordner. Für den Zugriff von unterwegs eignet sich ein VPN wie Tailscale.
        </p>
      </div>
      <Switch
        label="Zugriff vom Handy erlauben"
        description={`Öffnet Port ${settings.remote_view_port} im lokalen Netzwerk. macOS fragt beim ersten Mal, ob eingehende Verbindungen erlaubt werden sollen.`}
        checked={settings.remote_view_enabled}
        disabled={busy}
        onChange={toggle}
      />
      {settings.remote_view_enabled && info && (
        <div className="rounded-lg border border-line p-4">
          {info.error && <p className="mb-3 text-xs text-err">{info.error}</p>}
          {info.qr_svg ? (
            <div className="flex flex-col items-center gap-3 sm:flex-row sm:items-start">
              <div
                className="h-[220px] w-[220px] shrink-0 overflow-hidden rounded-lg bg-white p-1 [&_svg]:h-full [&_svg]:w-full"
                aria-label="QR-Code für die Handy-Ansicht"
                // Generated locally by the backend (qrcode crate), contains no user input.
                dangerouslySetInnerHTML={{ __html: info.qr_svg }}
              />
              <div className="min-w-0 space-y-2 text-xs">
                <p className="text-[13px] font-medium">Mit der Handy-Kamera scannen</p>
                <p className="text-muted">Auf dem iPhone in Safari „Teilen → Zum Home-Bildschirm“ wählen, dann ist die Ansicht wie eine App erreichbar.</p>
                <p className="break-all font-mono text-[11px] text-muted">{info.urls[0]}</p>
                <p className={info.running ? "text-ok" : "text-warn"}>{info.running ? "● Bereit" : "● Nicht aktiv"}</p>
                <Button size="sm" variant="ghost" onClick={resetToken}>
                  <RefreshCw size={13} /> Neuen Schlüssel erstellen
                </Button>
              </div>
            </div>
          ) : (
            <p className="text-xs text-warn">Keine Netzwerkverbindung gefunden. Ist der Mac mit einem WLAN verbunden?</p>
          )}
        </div>
      )}
    </div>
  );
}
