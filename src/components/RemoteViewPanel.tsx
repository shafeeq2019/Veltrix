import { RefreshCw, Smartphone } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { errorText, useT } from "../i18n";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import type { RemoteInfo } from "../lib/types";
import { Button, Switch } from "./ui";

/** Settings tab for the read-only phone status page. Changes apply immediately. */
export function RemoteViewPanel() {
  const t = useT();
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
      toast("error", errorText(e, t));
    } finally {
      setBusy(false);
    }
  };

  const resetToken = async () => {
    try {
      setSettings(await api.remoteViewResetToken());
      toast("success", t("remote.newKeyDone"));
    } catch (e) {
      toast("error", errorText(e, t));
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-start gap-3 rounded-lg border border-line bg-panel-2 p-3 text-xs text-muted">
        <Smartphone size={18} className="mt-0.5 shrink-0 text-accent" />
        <p>{t("remote.intro")}</p>
      </div>
      <Switch
        label={t("remote.enable")}
        description={t("remote.enableHint", { port: settings.remote_view_port })}
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
                aria-label={t("remote.qrLabel")}
                // Generated locally by the backend (qrcode crate), contains no user input.
                dangerouslySetInnerHTML={{ __html: info.qr_svg }}
              />
              <div className="min-w-0 space-y-2 text-xs">
                <p className="text-[13px] font-medium">{t("remote.scan")}</p>
                <p className="text-muted">{t("remote.homeScreen")}</p>
                <p className="break-all font-mono text-[11px] text-muted">{info.urls[0]}</p>
                <p className={info.running ? "text-ok" : "text-warn"}>{info.running ? t("remote.ready") : t("remote.inactive")}</p>
                <Button size="sm" variant="ghost" onClick={resetToken}>
                  <RefreshCw size={13} /> {t("remote.newKey")}
                </Button>
              </div>
            </div>
          ) : (
            <p className="text-xs text-warn">{t("remote.noNetwork")}</p>
          )}
        </div>
      )}
    </div>
  );
}
