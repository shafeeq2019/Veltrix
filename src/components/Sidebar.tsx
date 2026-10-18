import clsx from "clsx";
import { AlertTriangle, ArrowDownToLine, CheckCircle2, Clock, Folder, Layers, Loader } from "lucide-react";
import { useMemo } from "react";
import { categoryCounts, STATUS_CATEGORIES, TYPE_CATEGORIES, type CategoryKey } from "../lib/filter";
import { useStore } from "../lib/store";
import type { CustomCategory } from "../lib/types";
import { FileIcon } from "./FileIcon";

// Stable fallback: a new [] per call would make the selector loop forever.
const NO_CATEGORIES: CustomCategory[] = [];

const STATUS_ICONS: Record<string, typeof Layers> = {
  all: Layers,
  active: Loader,
  completed: CheckCircle2,
  queue: Clock,
  failed: AlertTriangle,
};

export function Sidebar() {
  const downloads = useStore((s) => s.downloads);
  const category = useStore((s) => s.category);
  const setCategory = useStore((s) => s.setCategory);
  const custom = useStore((s) => s.settings?.custom_categories ?? NO_CATEGORIES);
  const counts = useMemo(() => categoryCounts(Object.values(downloads), custom), [downloads, custom]);

  const item = (key: CategoryKey, label: string, icon: React.ReactNode) => (
    <li key={key}>
      <button
        onClick={() => setCategory(key)}
        aria-current={category === key ? "page" : undefined}
        className={clsx(
          "flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-[13px] transition-colors",
          category === key ? "bg-accent-soft font-medium text-accent" : "text-fg hover:bg-panel-2",
        )}
      >
        {icon}
        <span className="flex-1 truncate">{label}</span>
        <span className="tabular text-xs text-muted">{counts[key] ?? 0}</span>
      </button>
    </li>
  );

  return (
    <nav className="flex h-full w-56 shrink-0 flex-col border-r border-line bg-panel" aria-label="Kategorien">
      <div className="flex h-12 items-center gap-2 px-4">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-blue-600 to-cyan-600 text-white">
          <ArrowDownToLine size={16} />
        </div>
        <span className="font-semibold tracking-tight">OpenDownload</span>
      </div>
      <div className="flex-1 overflow-y-auto px-2 pb-3">
        <ul className="space-y-0.5">
          {STATUS_CATEGORIES.map((c) => {
            const Icon = STATUS_ICONS[c.key];
            return item(c.key, c.label, <Icon size={16} className="shrink-0 text-muted" aria-hidden />);
          })}
        </ul>
        <div className="mt-4 mb-1 px-2.5 text-[11px] font-semibold tracking-wide text-muted uppercase">Kategorien</div>
        <ul className="space-y-0.5">
          {TYPE_CATEGORIES.map((c) => item(c.key, c.label, <FileIcon category={c.key} size={16} />))}
          {custom.map((c) =>
            item(`custom:${c.id}` as CategoryKey, c.name, <Folder size={16} className="shrink-0 text-muted" aria-hidden />),
          )}
        </ul>
      </div>
    </nav>
  );
}
