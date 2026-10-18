import { Activity, AlertTriangle, CheckCircle2, Gauge } from "lucide-react";
import { useMemo } from "react";
import { formatSpeed } from "../lib/format";
import { useStore } from "../lib/store";

function Stat({ icon, label, value, tone }: { icon: React.ReactNode; label: string; value: string; tone: string }) {
  return (
    <div className="flex min-w-0 flex-1 items-center gap-3 rounded-lg border border-line bg-panel px-3.5 py-2.5">
      <div className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-md ${tone}`}>{icon}</div>
      <div className="min-w-0">
        <div className="truncate text-[11px] font-medium tracking-wide text-muted uppercase">{label}</div>
        <div className="tabular truncate text-[15px] font-semibold">{value}</div>
      </div>
    </div>
  );
}

export function StatsBar() {
  const downloads = useStore((s) => s.downloads);
  const counts = useMemo(() => {
    const all = Object.values(downloads);
    const running = all.filter((d) => d.status === "downloading" || d.status === "connecting");
    return {
      active: running.length,
      speed: running.reduce((sum, d) => sum + d.speed, 0),
      completed: all.filter((d) => d.status === "completed").length,
      failed: all.filter((d) => d.status === "failed").length,
    };
  }, [downloads]);

  return (
    <section className="grid shrink-0 grid-cols-2 gap-2 px-3 pt-3 lg:grid-cols-4" aria-label="Übersicht">
      <Stat icon={<Gauge size={17} />} tone="bg-accent-soft text-accent" label="Gesamtgeschwindigkeit" value={formatSpeed(counts.speed)} />
      <Stat icon={<Activity size={17} />} tone="bg-sky-500/12 text-sky-600" label="Aktive Downloads" value={String(counts.active)} />
      <Stat icon={<CheckCircle2 size={17} />} tone="bg-ok/12 text-ok" label="Abgeschlossen" value={String(counts.completed)} />
      <Stat icon={<AlertTriangle size={17} />} tone="bg-err/12 text-err" label="Fehlgeschlagen" value={String(counts.failed)} />
    </section>
  );
}
