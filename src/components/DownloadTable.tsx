import * as ContextMenu from "@radix-ui/react-context-menu";
import clsx from "clsx";
import { ArrowDown, ArrowUp, ChevronDown, ChevronUp, Inbox } from "lucide-react";
import type { ReactNode } from "react";
import { useI18n, type MessageKey } from "../i18n";
import { actions, canCancel, canPause, canResume } from "../lib/actions";
import type { Sort, SortKey } from "../lib/filter";
import { formatBytes, formatDuration, formatPercent, formatSpeed, percent } from "../lib/format";
import { statusText, statusTone } from "../lib/status";
import { useStore } from "../lib/store";
import type { DownloadView } from "../lib/types";
import { FileIcon } from "./FileIcon";
import { ProgressBar } from "./ui";

interface Props {
  rows: DownloadView[];
  onOpen: (d: DownloadView) => void;
  onRemove: (ids: string[]) => void;
  onNew: () => void;
}

const COLUMNS: { key: SortKey; label: MessageKey; className: string }[] = [
  { key: "name", label: "table.name", className: "flex-1 min-w-0" },
  { key: "size", label: "common.size", className: "w-36 hidden md:block" },
  { key: "progress", label: "table.progress", className: "w-40" },
  { key: "speed", label: "table.speed", className: "w-28 hidden lg:block" },
  { key: "status", label: "table.status", className: "w-44" },
];

function HeaderCell({ col, sort, setSort }: { col: (typeof COLUMNS)[number]; sort: Sort; setSort: (s: Sort) => void }) {
  const { t } = useI18n();
  const active = sort.key === col.key;
  return (
    <button
      className={clsx("flex items-center gap-1 truncate text-left hover:text-fg", col.className, active && "text-fg")}
      onClick={() => setSort({ key: col.key, dir: active && sort.dir === "desc" ? "asc" : "desc" })}
      aria-sort={active ? (sort.dir === "asc" ? "ascending" : "descending") : "none"}
    >
      {t(col.label)}
      {active && (sort.dir === "asc" ? <ChevronUp size={12} /> : <ChevronDown size={12} />)}
    </button>
  );
}

function Row({ d, selected, onSelect, onOpen }: {
  d: DownloadView;
  selected: boolean;
  onSelect: (e: React.MouseEvent) => void;
  onOpen: () => void;
}) {
  const { t, locale } = useI18n();
  const pct = d.status === "completed" ? 100 : percent(d.downloaded, d.total_size);
  const active = d.status === "downloading" || d.status === "connecting";
  return (
    <div
      role="row"
      aria-selected={selected}
      onMouseDown={(e) => {
        if (e.button === 0) onSelect(e);
      }}
      onContextMenu={(e) => {
        if (!selected) onSelect(e);
      }}
      onDoubleClick={onOpen}
      className={clsx(
        "flex h-14 cursor-default items-center gap-4 border-b border-line px-4 transition-colors",
        selected ? "bg-accent-soft" : "hover:bg-panel-2",
      )}
    >
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <FileIcon category={d.category} size={22} />
        <div className="min-w-0">
          <div className="truncate font-medium" title={d.file_name}>
            {d.file_name}
          </div>
          <div className="flex items-center gap-1.5 truncate text-xs text-muted">
            <span className="truncate">{d.domain ?? ""}</span>
            {d.in_queue && <span className="rounded bg-panel-2 px-1.5 text-[10px] font-medium">{t("common.queue")}</span>}
            {d.priority !== "normal" && (
              <span className="rounded bg-panel-2 px-1.5 text-[10px] font-medium">{d.priority === "high" ? t("common.priority.high") : t("common.priority.low")}</span>
            )}
          </div>
        </div>
      </div>
      <div className="tabular hidden w-36 truncate text-xs text-muted md:block">
        {d.status === "completed" ? formatBytes(d.total_size, locale) : `${formatBytes(d.downloaded, locale)} / ${formatBytes(d.total_size, locale)}`}
      </div>
      <div className="w-40">
        <ProgressBar value={pct} active={active} error={d.status === "failed"} />
        <div className="tabular mt-1 text-[11px] text-muted">{pct === null ? (active ? t("table.sizeUnknown") : "–") : formatPercent(pct, locale)}</div>
      </div>
      <div className="tabular hidden w-28 text-xs lg:block">{active ? formatSpeed(d.speed, locale) : <span className="text-muted">–</span>}</div>
      <div className="w-44 min-w-0">
        <div className={clsx("truncate text-xs font-medium", statusTone(d.status))} title={d.error?.message}>
          {statusText(d, t)}
        </div>
        <div className="tabular truncate text-[11px] text-muted">
          {active && d.eta_secs !== null
            ? t("table.remaining", { time: formatDuration(d.eta_secs) })
            : d.status === "completed" && d.avg_speed > 0
              ? t("table.average", { speed: formatSpeed(d.avg_speed, locale) })
              : ""}
        </div>
      </div>
    </div>
  );
}

function MenuItem({ children, onSelect, disabled, danger }: { children: ReactNode; onSelect: () => void; disabled?: boolean; danger?: boolean }) {
  return (
    <ContextMenu.Item
      disabled={disabled}
      onSelect={onSelect}
      className={clsx(
        "flex h-7 cursor-default items-center rounded px-2.5 text-[13px] outline-none select-none",
        "data-[disabled]:opacity-40 data-[highlighted]:bg-accent data-[highlighted]:text-accent-fg",
        danger && "text-err",
      )}
    >
      {children}
    </ContextMenu.Item>
  );
}

const menuClass = "z-50 min-w-52 rounded-lg border border-line bg-panel p-1 shadow-xl animate-pop-in";

export function DownloadTable({ rows, onOpen, onRemove, onNew }: Props) {
  const { t } = useI18n();
  const selectedIds = useStore((s) => s.selected);
  const select = useStore((s) => s.select);
  const sort = useStore((s) => s.sort);
  const setSort = useStore((s) => s.setSort);
  const visibleIds = rows.map((r) => r.id);
  const selected = rows.filter((r) => selectedIds.includes(r.id));
  const single = selected.length === 1 ? selected[0] : null;
  const ids = selected.map((d) => d.id);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-panel" role="grid" aria-label="Downloads" aria-multiselectable>
      <div className="flex h-9 shrink-0 items-center gap-4 border-b border-line px-4 text-[11px] font-semibold tracking-wide text-muted uppercase" role="row">
        {COLUMNS.map((c) => (
          <HeaderCell key={c.key} col={c} sort={sort} setSort={setSort} />
        ))}
      </div>
      <ContextMenu.Root>
        <ContextMenu.Trigger asChild disabled={rows.length === 0}>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {rows.length === 0 ? (
              <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center text-muted">
                <Inbox size={36} strokeWidth={1.5} />
                <div>
                  <div className="font-medium text-fg">{t("table.empty.title")}</div>
                  <div className="text-xs">{t("table.empty.body")}</div>
                </div>
                <button onClick={onNew} className="text-xs font-medium text-accent hover:underline">
                  {t("table.empty.action")}
                </button>
              </div>
            ) : (
              rows.map((d) => (
                <Row
                  key={d.id}
                  d={d}
                  selected={selectedIds.includes(d.id)}
                  onSelect={(e) => select(d.id, e.shiftKey ? "range" : e.metaKey || e.ctrlKey ? "toggle" : "single", visibleIds)}
                  onOpen={() => onOpen(d)}
                />
              ))
            )}
          </div>
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Content className={menuClass}>
            <MenuItem disabled={!single || single.status !== "completed"} onSelect={() => single && onOpen(single)}>
              {t("table.menu.open")}
            </MenuItem>
            <MenuItem disabled={!single} onSelect={() => single && actions.showInFolder(single.id)}>
              {t("table.menu.showInFolder")}
            </MenuItem>
            <ContextMenu.Separator className="my-1 h-px bg-line" />
            <MenuItem disabled={!selected.some(canResume)} onSelect={() => actions.resume(selected.filter(canResume).map((d) => d.id))}>
              {t("common.resume")}
            </MenuItem>
            <MenuItem disabled={!selected.some(canPause)} onSelect={() => actions.pause(selected.filter(canPause).map((d) => d.id))}>
              {t("common.pause")}
            </MenuItem>
            <MenuItem disabled={!selected.some(canCancel)} onSelect={() => actions.cancel(selected.filter(canCancel).map((d) => d.id))}>
              {t("common.cancel")}
            </MenuItem>
            <MenuItem
              disabled={!selected.some((d) => d.status !== "downloading" && d.status !== "connecting")}
              onSelect={() => actions.retry(selected.filter((d) => d.status !== "downloading" && d.status !== "connecting").map((d) => d.id))}
            >
              {t("table.menu.retry")}
            </MenuItem>
            <ContextMenu.Separator className="my-1 h-px bg-line" />
            <MenuItem disabled={!selected.some((d) => !d.in_queue && d.status !== "completed")} onSelect={() => actions.addToQueue(ids)}>
              {t("common.addToQueue")}
            </MenuItem>
            <MenuItem disabled={!selected.some((d) => d.in_queue)} onSelect={() => actions.removeFromQueue(ids)}>
              {t("table.menu.removeFromQueue")}
            </MenuItem>
            <MenuItem disabled={!single?.in_queue} onSelect={() => single && actions.moveInQueue(single.id, -1)}>
              <ArrowUp size={14} className="mr-2" /> {t("table.menu.moveUp")}
            </MenuItem>
            <MenuItem disabled={!single?.in_queue} onSelect={() => single && actions.moveInQueue(single.id, 1)}>
              <ArrowDown size={14} className="mr-2" /> {t("table.menu.moveDown")}
            </MenuItem>
            <ContextMenu.Sub>
              <ContextMenu.SubTrigger className="flex h-7 cursor-default items-center rounded px-2.5 text-[13px] outline-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-fg data-[state=open]:bg-panel-2">
                {t("table.menu.priority")}
              </ContextMenu.SubTrigger>
              <ContextMenu.Portal>
                <ContextMenu.SubContent className={menuClass}>
                  <MenuItem onSelect={() => actions.setPriority(ids, "high")}>{t("common.priority.high")}</MenuItem>
                  <MenuItem onSelect={() => actions.setPriority(ids, "normal")}>{t("common.priority.normal")}</MenuItem>
                  <MenuItem onSelect={() => actions.setPriority(ids, "low")}>{t("common.priority.low")}</MenuItem>
                </ContextMenu.SubContent>
              </ContextMenu.Portal>
            </ContextMenu.Sub>
            <ContextMenu.Separator className="my-1 h-px bg-line" />
            <MenuItem disabled={!single} onSelect={() => single && actions.copyUrl(single)}>
              {t("table.menu.copyUrl")}
            </MenuItem>
            <ContextMenu.Separator className="my-1 h-px bg-line" />
            <MenuItem danger disabled={ids.length === 0} onSelect={() => onRemove(ids)}>
              {t("table.menu.remove")}
            </MenuItem>
          </ContextMenu.Content>
        </ContextMenu.Portal>
      </ContextMenu.Root>
    </div>
  );
}
