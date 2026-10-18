import { Pause, Play, Plus, RotateCcw, Search, Settings as SettingsIcon, Square, Trash2, ListStart, ListX } from "lucide-react";
import { forwardRef } from "react";
import { actions, canCancel, canPause, canResume } from "../lib/actions";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import type { DownloadView } from "../lib/types";
import { Button, IconButton } from "./ui";

interface Props {
  selection: DownloadView[];
  onNew: () => void;
  onSettings: () => void;
  onRemove: (ids: string[]) => void;
}

export const Toolbar = forwardRef<HTMLInputElement, Props>(function Toolbar({ selection, onNew, onSettings, onRemove }, searchRef) {
  const search = useStore((s) => s.search);
  const setSearch = useStore((s) => s.setSearch);
  const queueRunning = useStore((s) => s.queueRunning);
  const setQueueRunning = useStore((s) => s.setQueueRunning);
  const ids = selection.map((d) => d.id);

  const toggleQueue = async () => {
    if (queueRunning) await api.stopQueue();
    else await api.startQueue();
    setQueueRunning(!queueRunning);
  };

  return (
    <div className="flex h-12 shrink-0 items-center gap-1 border-b border-line bg-panel px-3">
      <Button variant="primary" onClick={onNew} title="Neuer Download (Strg+N)">
        <Plus size={16} /> Neuer Download
      </Button>
      <div className="mx-2 h-5 w-px bg-line" />
      <IconButton label="Fortsetzen" disabled={!selection.some(canResume)} onClick={() => actions.resume(selection.filter(canResume).map((d) => d.id))}>
        <Play size={16} />
      </IconButton>
      <IconButton label="Pausieren" disabled={!selection.some(canPause)} onClick={() => actions.pause(selection.filter(canPause).map((d) => d.id))}>
        <Pause size={16} />
      </IconButton>
      <IconButton label="Abbrechen" disabled={!selection.some(canCancel)} onClick={() => actions.cancel(selection.filter(canCancel).map((d) => d.id))}>
        <Square size={15} />
      </IconButton>
      <IconButton
        label="Erneut versuchen"
        disabled={!selection.some((d) => d.status === "failed" || d.status === "canceled")}
        onClick={() => actions.retry(selection.filter((d) => d.status === "failed" || d.status === "canceled").map((d) => d.id))}
      >
        <RotateCcw size={16} />
      </IconButton>
      <IconButton label="Entfernen (Entf)" disabled={ids.length === 0} onClick={() => onRemove(ids)}>
        <Trash2 size={16} />
      </IconButton>
      <div className="mx-2 h-5 w-px bg-line" />
      <Button variant={queueRunning ? "secondary" : "ghost"} onClick={toggleQueue} title="Warteschlange starten oder anhalten">
        {queueRunning ? <ListX size={16} /> : <ListStart size={16} />}
        {queueRunning ? "Warteschlange anhalten" : "Warteschlange starten"}
      </Button>
      <div className="flex-1" />
      <div className="relative w-56 max-w-[30vw]">
        <Search size={14} className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted" aria-hidden />
        <input
          ref={searchRef}
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Suchen (Strg+F)"
          aria-label="Downloads durchsuchen"
          className="h-8 w-full rounded-md border border-line bg-panel-2 pr-2 pl-8 text-[13px] text-fg placeholder:text-muted focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25"
        />
      </div>
      <IconButton label="Einstellungen (Strg+,)" onClick={onSettings}>
        <SettingsIcon size={16} />
      </IconButton>
    </div>
  );
});
