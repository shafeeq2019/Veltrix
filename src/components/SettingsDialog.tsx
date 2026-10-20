import { open as openDialog } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { CheckCircle2, CircleDashed, FolderOpen, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { errorText, LANGUAGES, useT, type MessageKey } from "../i18n";
import { api } from "../lib/api";
import { fromBytesPerSecond, toBytesPerSecond, type SpeedUnit } from "../lib/format";
import { useStore } from "../lib/store";
import type { IntegrationStatus, LanguageSetting, Settings, SpeedMode } from "../lib/types";
import { RemoteViewPanel } from "./RemoteViewPanel";
import { Button, Dialog, Field, Input, Select, Switch } from "./ui";

type Tab = "general" | "speed" | "queue" | "browser" | "remote" | "categories";
const TABS: Tab[] = ["general", "speed", "queue", "browser", "remote", "categories"];
const DAYS = [0, 1, 2, 3, 4, 5, 6];

const listToText = (l: string[]) => l.join(", ");
const textToList = (t: string) =>
  t
    .split(/[,\n]/)
    .map((s) => s.trim())
    .filter(Boolean);

function ListField({ label, hint, value, onChange }: { label: string; hint?: string; value: string[]; onChange: (v: string[]) => void }) {
  const [text, setText] = useState(listToText(value));
  useEffect(() => setText(listToText(value)), [value]);
  return (
    <Field label={label} hint={hint}>
      <textarea
        value={text}
        rows={2}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => onChange(textToList(text))}
        className="w-full resize-y rounded-md border border-line bg-panel px-2.5 py-1.5 text-xs text-fg focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25"
      />
    </Field>
  );
}

function NumberField({ label, hint, value, min, max, onChange }: { label: string; hint?: string; value: number; min: number; max: number; onChange: (v: number) => void }) {
  return (
    <Field label={label} hint={hint}>
      <Input type="number" min={min} max={max} value={value} onChange={(e) => onChange(Math.min(max, Math.max(min, Number(e.target.value) || min)))} />
    </Field>
  );
}

export function SettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const current = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);
  const toast = useStore((s) => s.toast);
  const [tab, setTab] = useState<Tab>("general");
  const [draft, setDraft] = useState<Settings | null>(current);
  const [limitValue, setLimitValue] = useState(5);
  const [limitUnit, setLimitUnit] = useState<SpeedUnit>("MB/s");
  const [integration, setIntegration] = useState<IntegrationStatus | null>(null);

  useEffect(() => {
    if (!open || !current) return;
    setDraft(structuredClone(current));
    const l = fromBytesPerSecond(current.speed_limit_bps);
    setLimitValue(l.value);
    setLimitUnit(l.unit);
    api.integrationStatus().then(setIntegration).catch(() => setIntegration(null));
    // Only when the dialog opens: later settings updates (e.g. from the phone
    // tab) must not discard unsaved edits in the draft.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, current === null]);

  if (!draft) return null;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setDraft({ ...draft, [k]: v });
  const setCapture = <K extends keyof Settings["capture"]>(k: K, v: Settings["capture"][K]) =>
    setDraft({ ...draft, capture: { ...draft.capture, [k]: v } });
  const setSchedule = <K extends keyof Settings["schedule"]>(k: K, v: Settings["schedule"][K]) =>
    setDraft({ ...draft, schedule: { ...draft.schedule, [k]: v } });

  const save = async () => {
    try {
      const latest = useStore.getState().settings;
      const next = {
        ...draft,
        speed_limit_bps: toBytesPerSecond(limitValue, limitUnit) || draft.speed_limit_bps,
        // The phone tab saves immediately; keep its values instead of the older draft.
        remote_view_enabled: latest?.remote_view_enabled ?? draft.remote_view_enabled,
        remote_view_port: latest?.remote_view_port ?? draft.remote_view_port,
        remote_view_token: latest?.remote_view_token ?? draft.remote_view_token,
      };
      const saved = await api.updateSettings(next);
      setSettings(saved);
      toast("success", t("settings.saved"));
      onClose();
    } catch (e) {
      toast("error", errorText(e, t));
    }
  };

  // The language is saved right away so the dialog switches with it.
  const setLanguage = async (language: LanguageSetting) => {
    setDraft({ ...draft, language });
    const latest = useStore.getState().settings;
    if (!latest) return;
    try {
      setSettings(await api.updateSettings({ ...latest, language }));
    } catch (e) {
      toast("error", errorText(e, t));
    }
  };

  const chooseDir = async () => {
    const dir = await openDialog({ directory: true, defaultPath: draft.download_dir, title: t("settings.downloadDir") });
    if (typeof dir === "string") set("download_dir", dir);
  };

  const integrationAction = async (register: boolean) => {
    try {
      setIntegration(register ? await api.registerIntegration() : await api.unregisterIntegration());
    } catch (e) {
      toast("error", errorText(e, t));
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title={t("settings.title")}
      width="max-w-3xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" onClick={save}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="flex min-h-[420px] gap-5">
        <div className="flex w-40 shrink-0 flex-col gap-0.5" role="tablist" aria-orientation="vertical">
          {TABS.map((key) => (
            <button
              key={key}
              role="tab"
              aria-selected={tab === key}
              onClick={() => setTab(key)}
              className={clsx("h-8 rounded-md px-2.5 text-left text-[13px]", tab === key ? "bg-accent-soft font-medium text-accent" : "hover:bg-panel-2")}
            >
              {t(`settings.tab.${key}`)}
            </button>
          ))}
        </div>
        <div className="min-w-0 flex-1 space-y-3" role="tabpanel">
          {tab === "general" && (
            <>
              <Field label={t("settings.downloadDir")}>
                <div className="flex gap-2">
                  <Input value={draft.download_dir} onChange={(e) => set("download_dir", e.target.value)} />
                  <Button onClick={chooseDir}>
                    <FolderOpen size={14} /> {t("common.change")}
                  </Button>
                </div>
              </Field>
              <Field label={t("settings.language")}>
                <Select value={draft.language} onChange={(e) => setLanguage(e.target.value as LanguageSetting)}>
                  <option value="system">{t("settings.language.system")}</option>
                  {LANGUAGES.map((l) => (
                    <option key={l.code} value={l.code} lang={l.code}>
                      {l.name}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label={t("settings.theme")}>
                <Select value={draft.theme} onChange={(e) => set("theme", e.target.value as Settings["theme"])}>
                  <option value="system">{t("settings.theme.system")}</option>
                  <option value="light">{t("settings.theme.light")}</option>
                  <option value="dark">{t("settings.theme.dark")}</option>
                </Select>
              </Field>
              <Switch label={t("settings.minimizeToTray")} description={t("settings.minimizeToTrayHint")} checked={draft.minimize_to_tray} onChange={(v) => set("minimize_to_tray", v)} />
              <Switch label={t("settings.notifyComplete")} checked={draft.notify_on_complete} onChange={(v) => set("notify_on_complete", v)} />
              <Switch label={t("settings.notifyError")} checked={draft.notify_on_error} onChange={(v) => set("notify_on_error", v)} />
              <Switch
                label={t("settings.preventSleep")}
                description={t("settings.preventSleepHint")}
                checked={draft.prevent_sleep}
                onChange={(v) => set("prevent_sleep", v)}
              />
              <Switch label={t("settings.resumeOnStartup")} checked={draft.resume_on_startup} onChange={(v) => set("resume_on_startup", v)} />
              <Switch label={t("settings.removeCompleted")} description={t("settings.removeCompletedHint")} checked={draft.remove_completed_from_list} onChange={(v) => set("remove_completed_from_list", v)} />
            </>
          )}

          {tab === "speed" && (
            <>
              <fieldset className="space-y-2">
                <legend className="mb-1 text-xs font-medium text-muted">{t("settings.speedMode")}</legend>
                {(
                  [
                    ["maximum", "settings.speed.maximum", "settings.speed.maximumHint"],
                    ["balanced", "settings.speed.balanced", "settings.speed.balancedHint"],
                    ["custom", "settings.speed.custom", "settings.speed.customHint"],
                  ] as [SpeedMode, MessageKey, MessageKey][]
                ).map(([mode, label, desc]) => (
                  <label key={mode} className={clsx("flex cursor-pointer gap-3 rounded-lg border p-3", draft.speed_mode === mode ? "border-accent bg-accent-soft/50" : "border-line")}>
                    <input type="radio" name="speed" checked={draft.speed_mode === mode} onChange={() => set("speed_mode", mode)} className="mt-0.5 accent-[var(--odm-accent)]" />
                    <span>
                      <span className="block font-medium">{t(label)}</span>
                      <span className="block text-xs text-muted">{t(desc)}</span>
                    </span>
                  </label>
                ))}
              </fieldset>
              {draft.speed_mode === "custom" && (
                <Field label={t("settings.speedLimit")}>
                  <div className="flex gap-2">
                    <Input type="number" min={1} step="any" value={limitValue} onChange={(e) => setLimitValue(Number(e.target.value))} className="w-32" />
                    <Select value={limitUnit} onChange={(e) => setLimitUnit(e.target.value as SpeedUnit)}>
                      <option value="KB/s">KB/s</option>
                      <option value="MB/s">MB/s</option>
                    </Select>
                  </div>
                </Field>
              )}
              <div className="grid grid-cols-2 gap-3">
                <NumberField label={t("settings.segments")} hint={t("settings.segmentsHint")} value={draft.segments_per_download} min={1} max={32} onChange={(v) => set("segments_per_download", v)} />
                <NumberField label={t("settings.totalConnections")} value={draft.max_total_connections} min={1} max={128} onChange={(v) => set("max_total_connections", v)} />
                <NumberField label={t("settings.retries")} value={draft.max_retries} min={0} max={50} onChange={(v) => set("max_retries", v)} />
                <NumberField label={t("settings.readTimeout")} value={draft.read_timeout_secs} min={5} max={600} onChange={(v) => set("read_timeout_secs", v)} />
              </div>
              <Switch
                label={t("settings.insecureRedirects")}
                description={t("settings.insecureRedirectsHint")}
                checked={draft.allow_insecure_redirects}
                onChange={(v) => set("allow_insecure_redirects", v)}
              />
              <Field label={t("settings.userAgent")}>
                <Input value={draft.user_agent} onChange={(e) => set("user_agent", e.target.value)} />
              </Field>
            </>
          )}

          {tab === "queue" && (
            <>
              <NumberField label={t("settings.concurrent")} hint={t("settings.concurrentHint")} value={draft.max_concurrent_downloads} min={1} max={20} onChange={(v) => set("max_concurrent_downloads", v)} />
              <Switch label={t("settings.startQueueOnStartup")} checked={draft.start_queue_on_startup} onChange={(v) => set("start_queue_on_startup", v)} />
              <Switch label={t("settings.autoRetry")} description={t("settings.autoRetryHint")} checked={draft.auto_retry_failed} onChange={(v) => set("auto_retry_failed", v)} />
              <div className="grid grid-cols-2 gap-3">
                <NumberField label={t("settings.autoRetryDelay")} value={draft.auto_retry_delay_secs} min={5} max={86400} onChange={(v) => set("auto_retry_delay_secs", v)} />
                <NumberField label={t("settings.autoRetryMax")} value={draft.auto_retry_max} min={0} max={100} onChange={(v) => set("auto_retry_max", v)} />
              </div>
              <div className="rounded-lg border border-line p-3">
                <Switch label={t("settings.schedule")} description={t("settings.scheduleHint")} checked={draft.schedule.enabled} onChange={(v) => setSchedule("enabled", v)} />
                <div className="mt-2 grid grid-cols-2 gap-3">
                  <Field label={t("settings.scheduleStart")}>
                    <Input type="time" value={draft.schedule.start_time} onChange={(e) => setSchedule("start_time", e.target.value)} />
                  </Field>
                  <Field label={t("settings.scheduleStop")}>
                    <Input type="time" value={draft.schedule.stop_time ?? ""} onChange={(e) => setSchedule("stop_time", e.target.value || null)} />
                  </Field>
                </div>
                <div className="mt-3 flex flex-wrap gap-1.5" role="group" aria-label={t("settings.weekdays")}>
                  {DAYS.map((i) => {
                    const on = draft.schedule.days.length === 0 || draft.schedule.days.includes(i);
                    return (
                      <button
                        key={i}
                        aria-pressed={on}
                        onClick={() => {
                          const all = draft.schedule.days.length === 0 ? [0, 1, 2, 3, 4, 5, 6] : draft.schedule.days;
                          const next = on ? all.filter((x) => x !== i) : [...all, i].sort();
                          setSchedule("days", next.length === 7 ? [] : next);
                        }}
                        className={clsx("h-7 w-9 rounded-md border text-xs", on ? "border-accent bg-accent-soft text-accent" : "border-line text-muted")}
                      >
                        {t(`settings.day.${i}` as MessageKey)}
                      </button>
                    );
                  })}
                </div>
              </div>
            </>
          )}

          {tab === "browser" && (
            <>
              <Switch label={t("settings.capture")} description={t("settings.captureHint")} checked={draft.capture.enabled} onChange={(v) => setCapture("enabled", v)} />
              <Switch label={t("settings.autoStart")} description={t("settings.autoStartHint")} checked={draft.capture.auto_start} onChange={(v) => setCapture("auto_start", v)} />
              <Switch label={t("settings.capturePrivate")} checked={draft.capture.capture_private} onChange={(v) => setCapture("capture_private", v)} />
              <Switch label={t("settings.sendReferrer")} description={t("settings.sendReferrerHint")} checked={draft.capture.send_referrer} onChange={(v) => setCapture("send_referrer", v)} />
              <ListField label={t("settings.fileTypes")} value={draft.capture.file_types} onChange={(v) => setCapture("file_types", v)} />
              <ListField label={t("settings.alwaysTakeOver")} value={draft.capture.always_take_over} onChange={(v) => setCapture("always_take_over", v)} />
              <ListField label={t("settings.excludedDomains")} hint={t("settings.excludedDomainsHint")} value={draft.capture.excluded_domains} onChange={(v) => setCapture("excluded_domains", v)} />
              <ListField label={t("settings.excludedFileTypes")} value={draft.capture.excluded_file_types} onChange={(v) => setCapture("excluded_file_types", v)} />
              <ListField label={t("settings.excludedUrlPatterns")} value={draft.capture.excluded_url_patterns} onChange={(v) => setCapture("excluded_url_patterns", v)} />
              <NumberField label={t("settings.minSize")} value={Math.round(draft.capture.min_size_bytes / 1024)} min={0} max={10_000_000} onChange={(v) => setCapture("min_size_bytes", v * 1024)} />
              <ListField label={t("settings.extensionIds")} hint={t("settings.extensionIdsHint")} value={draft.chromium_extension_ids} onChange={(v) => set("chromium_extension_ids", v)} />
              <div className="rounded-lg border border-line p-3">
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-xs font-medium text-muted">{t("settings.nativeMessaging")}</span>
                  <div className="flex gap-2">
                    <Button size="sm" onClick={() => integrationAction(true)}>
                      {t("settings.register")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => integrationAction(false)}>
                      {t("common.remove")}
                    </Button>
                  </div>
                </div>
                {integration ? (
                  <ul className="space-y-1 text-xs">
                    {!integration.host_path && <li className="text-warn">{t("settings.hostMissing")}</li>}
                    {integration.browsers.map((b) => (
                      <li key={b.browser} className="flex items-center gap-2" title={b.location}>
                        {b.registered ? <CheckCircle2 size={13} className="text-ok" /> : <CircleDashed size={13} className="text-muted" />}
                        {b.browser}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <span className="text-xs text-muted">{t("settings.statusUnknown")}</span>
                )}
              </div>
            </>
          )}

          {tab === "remote" && <RemoteViewPanel />}

          {tab === "categories" && (
            <>
              <p className="text-xs text-muted">{t("settings.categoriesHint")}</p>
              {draft.custom_categories.map((c, idx) => (
                <div key={c.id} className="flex items-end gap-2 rounded-lg border border-line p-3">
                  <Field label={t("settings.categoryName")}>
                    <Input
                      value={c.name}
                      onChange={(e) => {
                        const next = [...draft.custom_categories];
                        next[idx] = { ...c, name: e.target.value };
                        set("custom_categories", next);
                      }}
                    />
                  </Field>
                  <div className="flex-1">
                    <ListField
                      label={t("settings.categoryExtensions")}
                      value={c.extensions}
                      onChange={(v) => {
                        const next = [...draft.custom_categories];
                        next[idx] = { ...c, extensions: v };
                        set("custom_categories", next);
                      }}
                    />
                  </div>
                  <Button variant="ghost" aria-label={t("settings.deleteCategory")} onClick={() => set("custom_categories", draft.custom_categories.filter((x) => x.id !== c.id))}>
                    <Trash2 size={14} />
                  </Button>
                </div>
              ))}
              <Button onClick={() => set("custom_categories", [...draft.custom_categories, { id: crypto.randomUUID().slice(0, 8), name: t("settings.newCategory"), extensions: [] }])}>
                <Plus size={14} /> {t("settings.addCategory")}
              </Button>
            </>
          )}
        </div>
      </div>
    </Dialog>
  );
}
