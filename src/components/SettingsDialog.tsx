import { open as openDialog } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { CheckCircle2, CircleDashed, FolderOpen, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../lib/api";
import { fromBytesPerSecond, toBytesPerSecond, type SpeedUnit } from "../lib/format";
import { useStore } from "../lib/store";
import type { IntegrationStatus, Settings, SpeedMode } from "../lib/types";
import { Button, Dialog, Field, Input, Select, Switch } from "./ui";

type Tab = "general" | "speed" | "queue" | "browser" | "categories";
const TABS: { key: Tab; label: string }[] = [
  { key: "general", label: "Allgemein" },
  { key: "speed", label: "Geschwindigkeit" },
  { key: "queue", label: "Warteschlange" },
  { key: "browser", label: "Browser" },
  { key: "categories", label: "Kategorien" },
];
const DAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

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
  }, [open, current]);

  if (!draft) return null;
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setDraft({ ...draft, [k]: v });
  const setCapture = <K extends keyof Settings["capture"]>(k: K, v: Settings["capture"][K]) =>
    setDraft({ ...draft, capture: { ...draft.capture, [k]: v } });
  const setSchedule = <K extends keyof Settings["schedule"]>(k: K, v: Settings["schedule"][K]) =>
    setDraft({ ...draft, schedule: { ...draft.schedule, [k]: v } });

  const save = async () => {
    try {
      const next = { ...draft, speed_limit_bps: toBytesPerSecond(limitValue, limitUnit) || draft.speed_limit_bps };
      const saved = await api.updateSettings(next);
      setSettings(saved);
      toast("success", "Einstellungen gespeichert");
      onClose();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  const chooseDir = async () => {
    const dir = await openDialog({ directory: true, defaultPath: draft.download_dir, title: "Standard-Downloadordner" });
    if (typeof dir === "string") set("download_dir", dir);
  };

  const integrationAction = async (register: boolean) => {
    try {
      setIntegration(register ? await api.registerIntegration() : await api.unregisterIntegration());
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title="Einstellungen"
      width="max-w-3xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Abbrechen
          </Button>
          <Button variant="primary" onClick={save}>
            Speichern
          </Button>
        </>
      }
    >
      <div className="flex min-h-[420px] gap-5">
        <div className="flex w-40 shrink-0 flex-col gap-0.5" role="tablist" aria-orientation="vertical">
          {TABS.map((t) => (
            <button
              key={t.key}
              role="tab"
              aria-selected={tab === t.key}
              onClick={() => setTab(t.key)}
              className={clsx("h-8 rounded-md px-2.5 text-left text-[13px]", tab === t.key ? "bg-accent-soft font-medium text-accent" : "hover:bg-panel-2")}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div className="min-w-0 flex-1 space-y-3" role="tabpanel">
          {tab === "general" && (
            <>
              <Field label="Standard-Downloadordner">
                <div className="flex gap-2">
                  <Input value={draft.download_dir} onChange={(e) => set("download_dir", e.target.value)} />
                  <Button onClick={chooseDir}>
                    <FolderOpen size={14} /> Ändern
                  </Button>
                </div>
              </Field>
              <Field label="Darstellung">
                <Select value={draft.theme} onChange={(e) => set("theme", e.target.value as Settings["theme"])}>
                  <option value="system">Wie System</option>
                  <option value="light">Hell</option>
                  <option value="dark">Dunkel</option>
                </Select>
              </Field>
              <Switch label="Beim Schließen in den Hintergrund minimieren" description="Die App läuft im System-Tray bzw. in der Menüleiste weiter." checked={draft.minimize_to_tray} onChange={(v) => set("minimize_to_tray", v)} />
              <Switch label="Benachrichtigung bei abgeschlossenen Downloads" checked={draft.notify_on_complete} onChange={(v) => set("notify_on_complete", v)} />
              <Switch label="Benachrichtigung bei Fehlern" checked={draft.notify_on_error} onChange={(v) => set("notify_on_error", v)} />
              <Switch label="Unterbrochene Downloads nach Neustart fortsetzen" checked={draft.resume_on_startup} onChange={(v) => set("resume_on_startup", v)} />
              <Switch label="Abgeschlossene Downloads automatisch aus der Liste entfernen" description="Die Dateien bleiben erhalten." checked={draft.remove_completed_from_list} onChange={(v) => set("remove_completed_from_list", v)} />
            </>
          )}

          {tab === "speed" && (
            <>
              <fieldset className="space-y-2">
                <legend className="mb-1 text-xs font-medium text-muted">Geschwindigkeitsmodus</legend>
                {(
                  [
                    ["maximum", "Maximale Geschwindigkeit", "Keine künstliche Begrenzung. Nutzt bis zu der eingestellten Anzahl Verbindungen, sofern der Server sie erlaubt."],
                    ["balanced", "Ausgewogen", "Höchstens 2 parallele Downloads mit je 4 Verbindungen und ca. 80 % der beobachteten Spitzenbandbreite – lässt Luft für andere Anwendungen."],
                    ["custom", "Benutzerdefinierte Begrenzung", "Feste Obergrenze für alle Downloads zusammen."],
                  ] as [SpeedMode, string, string][]
                ).map(([mode, label, desc]) => (
                  <label key={mode} className={clsx("flex cursor-pointer gap-3 rounded-lg border p-3", draft.speed_mode === mode ? "border-accent bg-accent-soft/50" : "border-line")}>
                    <input type="radio" name="speed" checked={draft.speed_mode === mode} onChange={() => set("speed_mode", mode)} className="mt-0.5 accent-[var(--odm-accent)]" />
                    <span>
                      <span className="block font-medium">{label}</span>
                      <span className="block text-xs text-muted">{desc}</span>
                    </span>
                  </label>
                ))}
              </fieldset>
              {draft.speed_mode === "custom" && (
                <Field label="Gesamtlimit">
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
                <NumberField label="Verbindungen pro Download" hint="Mehr Verbindungen helfen nur, wenn der Server sie zulässt." value={draft.segments_per_download} min={1} max={32} onChange={(v) => set("segments_per_download", v)} />
                <NumberField label="Verbindungen insgesamt" value={draft.max_total_connections} min={1} max={128} onChange={(v) => set("max_total_connections", v)} />
                <NumberField label="Wiederholungen pro Verbindung" value={draft.max_retries} min={0} max={50} onChange={(v) => set("max_retries", v)} />
                <NumberField label="Lese-Timeout (s)" value={draft.read_timeout_secs} min={5} max={600} onChange={(v) => set("read_timeout_secs", v)} />
              </div>
              <Switch
                label="Weiterleitungen von HTTPS auf HTTP erlauben"
                description="Nicht empfohlen: Die Datei würde unverschlüsselt übertragen."
                checked={draft.allow_insecure_redirects}
                onChange={(v) => set("allow_insecure_redirects", v)}
              />
              <Field label="User-Agent (leer = Standard)">
                <Input value={draft.user_agent} onChange={(e) => set("user_agent", e.target.value)} />
              </Field>
            </>
          )}

          {tab === "queue" && (
            <>
              <NumberField label="Gleichzeitige Downloads der Warteschlange" hint="1 = nacheinander" value={draft.max_concurrent_downloads} min={1} max={20} onChange={(v) => set("max_concurrent_downloads", v)} />
              <Switch label="Warteschlange beim App-Start starten" checked={draft.start_queue_on_startup} onChange={(v) => set("start_queue_on_startup", v)} />
              <Switch label="Fehlgeschlagene Downloads automatisch erneut versuchen" description="Nur bei vorübergehenden Fehlern (Netzwerk, Serverüberlastung)." checked={draft.auto_retry_failed} onChange={(v) => set("auto_retry_failed", v)} />
              <div className="grid grid-cols-2 gap-3">
                <NumberField label="Wartezeit bis zum neuen Versuch (s)" value={draft.auto_retry_delay_secs} min={5} max={86400} onChange={(v) => set("auto_retry_delay_secs", v)} />
                <NumberField label="Maximale automatische Versuche" value={draft.auto_retry_max} min={0} max={100} onChange={(v) => set("auto_retry_max", v)} />
              </div>
              <div className="rounded-lg border border-line p-3">
                <Switch label="Zeitplan aktivieren" description="Startet und stoppt die Warteschlange automatisch." checked={draft.schedule.enabled} onChange={(v) => setSchedule("enabled", v)} />
                <div className="mt-2 grid grid-cols-2 gap-3">
                  <Field label="Start">
                    <Input type="time" value={draft.schedule.start_time} onChange={(e) => setSchedule("start_time", e.target.value)} />
                  </Field>
                  <Field label="Stopp (optional)">
                    <Input type="time" value={draft.schedule.stop_time ?? ""} onChange={(e) => setSchedule("stop_time", e.target.value || null)} />
                  </Field>
                </div>
                <div className="mt-3 flex flex-wrap gap-1.5" role="group" aria-label="Wochentage">
                  {DAYS.map((d, i) => {
                    const on = draft.schedule.days.length === 0 || draft.schedule.days.includes(i);
                    return (
                      <button
                        key={d}
                        aria-pressed={on}
                        onClick={() => {
                          const all = draft.schedule.days.length === 0 ? [0, 1, 2, 3, 4, 5, 6] : draft.schedule.days;
                          const next = on ? all.filter((x) => x !== i) : [...all, i].sort();
                          setSchedule("days", next.length === 7 ? [] : next);
                        }}
                        className={clsx("h-7 w-9 rounded-md border text-xs", on ? "border-accent bg-accent-soft text-accent" : "border-line text-muted")}
                      >
                        {d}
                      </button>
                    );
                  })}
                </div>
              </div>
            </>
          )}

          {tab === "browser" && (
            <>
              <Switch label="Downloads im Browser erkennen" description="Erfordert die OpenDownload-Erweiterung für Chrome, Edge, Brave oder Firefox." checked={draft.capture.enabled} onChange={(v) => setCapture("enabled", v)} />
              <Switch label="Automatisch übernehmen" description="Erkannte Downloads ohne Rückfrage starten." checked={draft.capture.auto_start} onChange={(v) => setCapture("auto_start", v)} />
              <Switch label="Auch in privaten Fenstern erkennen" checked={draft.capture.capture_private} onChange={(v) => setCapture("capture_private", v)} />
              <Switch label="Referrer übermitteln" description="Manche Server liefern Dateien nur mit der Herkunftsseite aus." checked={draft.capture.send_referrer} onChange={(v) => setCapture("send_referrer", v)} />
              <ListField label="Erkannte Dateitypen" value={draft.capture.file_types} onChange={(v) => setCapture("file_types", v)} />
              <ListField label="Immer ohne Nachfrage übernehmen" value={draft.capture.always_take_over} onChange={(v) => setCapture("always_take_over", v)} />
              <ListField label="Ausgeschlossene Domains" hint="z. B. example.com (inkl. Subdomains) oder *.example.com" value={draft.capture.excluded_domains} onChange={(v) => setCapture("excluded_domains", v)} />
              <ListField label="Ausgeschlossene Dateitypen" value={draft.capture.excluded_file_types} onChange={(v) => setCapture("excluded_file_types", v)} />
              <ListField label="Ausgeschlossene URL-Bestandteile" value={draft.capture.excluded_url_patterns} onChange={(v) => setCapture("excluded_url_patterns", v)} />
              <NumberField label="Mindestgröße (KB)" value={Math.round(draft.capture.min_size_bytes / 1024)} min={0} max={10_000_000} onChange={(v) => setCapture("min_size_bytes", v * 1024)} />
              <ListField label="Zusätzliche Chromium-Erweiterungs-IDs" hint="Für Store-Versionen der Erweiterung." value={draft.chromium_extension_ids} onChange={(v) => set("chromium_extension_ids", v)} />
              <div className="rounded-lg border border-line p-3">
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-xs font-medium text-muted">Native-Messaging-Registrierung</span>
                  <div className="flex gap-2">
                    <Button size="sm" onClick={() => integrationAction(true)}>
                      Registrieren
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => integrationAction(false)}>
                      Entfernen
                    </Button>
                  </div>
                </div>
                {integration ? (
                  <ul className="space-y-1 text-xs">
                    {!integration.host_path && <li className="text-warn">Native-Messaging-Host nicht gefunden (Entwicklungsmodus?).</li>}
                    {integration.browsers.map((b) => (
                      <li key={b.browser} className="flex items-center gap-2" title={b.location}>
                        {b.registered ? <CheckCircle2 size={13} className="text-ok" /> : <CircleDashed size={13} className="text-muted" />}
                        {b.browser}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <span className="text-xs text-muted">Status unbekannt</span>
                )}
              </div>
            </>
          )}

          {tab === "categories" && (
            <>
              <p className="text-xs text-muted">Eigene Kategorien werden anhand der Dateiendung zugeordnet und vor den Standardkategorien geprüft.</p>
              {draft.custom_categories.map((c, idx) => (
                <div key={c.id} className="flex items-end gap-2 rounded-lg border border-line p-3">
                  <Field label="Name">
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
                      label="Dateiendungen"
                      value={c.extensions}
                      onChange={(v) => {
                        const next = [...draft.custom_categories];
                        next[idx] = { ...c, extensions: v };
                        set("custom_categories", next);
                      }}
                    />
                  </div>
                  <Button variant="ghost" aria-label="Kategorie löschen" onClick={() => set("custom_categories", draft.custom_categories.filter((x) => x.id !== c.id))}>
                    <Trash2 size={14} />
                  </Button>
                </div>
              ))}
              <Button onClick={() => set("custom_categories", [...draft.custom_categories, { id: crypto.randomUUID().slice(0, 8), name: "Neue Kategorie", extensions: [] }])}>
                <Plus size={14} /> Kategorie hinzufügen
              </Button>
            </>
          )}
        </div>
      </div>
    </Dialog>
  );
}
