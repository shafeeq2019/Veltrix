import clsx from "clsx";
import { AlertTriangle, CheckCircle2, Info, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AddDownloadDialog } from "./components/AddDownloadDialog";
import { CaptureDialog } from "./components/CaptureDialog";
import { DownloadTable } from "./components/DownloadTable";
import { SettingsDialog } from "./components/SettingsDialog";
import { Sidebar } from "./components/Sidebar";
import { StatsBar } from "./components/StatsBar";
import { Toolbar } from "./components/Toolbar";
import { Button, Dialog } from "./components/ui";
import { actions } from "./lib/actions";
import { errorText, translateNodes, useI18n, useT } from "./i18n";
import { api, isTauri, onCaptureOffer, onEngineEvent, onOfferExpired, onResync } from "./lib/api";
import { visibleDownloads } from "./lib/filter";
import { extractUrls } from "./lib/format";
import { useStore } from "./lib/store";
import type { DownloadView } from "./lib/types";

function useTheme() {
  const theme = useStore((s) => s.settings?.theme ?? "system");
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && media.matches);
      document.documentElement.classList.toggle("dark", dark);
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

function useDocumentLanguage() {
  const { lang } = useI18n();
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);
}

function Toasts() {
  const t = useT();
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-[60] flex w-96 max-w-[calc(100vw-32px)] flex-col gap-2" aria-live="polite">
      {toasts.map((item) => (
        <div key={item.id} className="pointer-events-auto flex items-start gap-2.5 rounded-lg border border-line bg-panel px-3.5 py-2.5 shadow-lg animate-pop-in">
          {item.kind === "error" ? <AlertTriangle size={16} className="mt-0.5 shrink-0 text-err" /> : item.kind === "success" ? <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-ok" /> : <Info size={16} className="mt-0.5 shrink-0 text-accent" />}
          <span className="flex-1 text-[13px] break-words">{item.text}</span>
          <button onClick={() => dismiss(item.id)} aria-label={t("app.toast.close")} className="text-muted hover:text-fg">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}

export default function App() {
  const downloads = useStore((s) => s.downloads);
  const category = useStore((s) => s.category);
  const search = useStore((s) => s.search);
  const sort = useStore((s) => s.sort);
  const selectedIds = useStore((s) => s.selected);
  const { setAll, applyEvent, setSettings, setQueueRunning, pushOffer, dropOffer, setSelected, toast } = useStore.getState();

  const [addOpen, setAddOpen] = useState(false);
  const [addUrl, setAddUrl] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [removeIds, setRemoveIds] = useState<string[] | null>(null);
  const [confirmExec, setConfirmExec] = useState<DownloadView | null>(null);
  const [dragging, setDragging] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const { t, lang } = useI18n();
  useTheme();
  useDocumentLanguage();

  const rows = useMemo(() => visibleDownloads(Object.values(downloads), category, search, sort), [downloads, category, search, sort]);
  const selection = useMemo(() => rows.filter((r) => selectedIds.includes(r.id)), [rows, selectedIds]);

  const reload = useCallback(async () => {
    try {
      setAll(await api.list());
      const info = await api.appInfo();
      setQueueRunning(info.queue_running);
    } catch (e) {
      toast("error", t("app.loadFailed", { error: errorText(e, t) }));
    }
  }, [setAll, setQueueRunning, toast, t]);

  useEffect(() => {
    if (!isTauri()) return;
    const unlisten: Promise<() => void>[] = [
      onEngineEvent(applyEvent),
      onResync(reload),
      onCaptureOffer(pushOffer),
      onOfferExpired(dropOffer),
    ];
    reload();
    api.getSettings().then(setSettings).catch(() => {});
    api.pendingOffers().then((o) => o.forEach(pushOffer)).catch(() => {});
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, [applyEvent, reload, pushOffer, dropOffer, setSettings]);

  const openAdd = (url = "") => {
    setAddUrl(url);
    setAddOpen(true);
  };

  const openFile = async (d: DownloadView) => {
    if (d.status !== "completed") {
      actions.toggle([d]);
      return;
    }
    const r = await actions.openFile(d.id, false);
    if (r === "confirm") setConfirmExec(d);
  };

  // Keyboard shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const typing = target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
      const mod = e.ctrlKey || e.metaKey;
      const dialogOpen = addOpen || settingsOpen || removeIds !== null || confirmExec !== null || useStore.getState().offers.length > 0;
      if (dialogOpen) return;
      if (mod && e.key.toLowerCase() === "n") {
        e.preventDefault();
        openAdd();
      } else if (mod && e.key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
      } else if (mod && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (typing) {
        return;
      } else if (mod && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setSelected(rows.map((r) => r.id));
      } else if (e.key === "Delete" || (e.key === "Backspace" && e.metaKey)) {
        if (selection.length) setRemoveIds(selection.map((d) => d.id));
      } else if (e.key === " ") {
        e.preventDefault();
        if (selection.length) actions.toggle(selection);
      } else if (e.key === "Enter" && selection.length === 1) {
        openFile(selection[0]);
      } else if (e.key === "Escape") {
        setSelected([]);
      }
    };
    const onPaste = (e: ClipboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || addOpen) return;
      const urls = extractUrls(e.clipboardData?.getData("text/plain") ?? "");
      if (urls.length) {
        e.preventDefault();
        openAdd(urls.join("\n"));
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("paste", onPaste);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("paste", onPaste);
    };
  });

  // Drag and drop of links (text/uri-list) from browsers.
  const onDragOver = (e: React.DragEvent) => {
    const types = Array.from(e.dataTransfer.types);
    if (types.includes("text/uri-list") || types.includes("text/plain")) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
      setDragging(true);
    }
  };
  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const text = e.dataTransfer.getData("text/uri-list") || e.dataTransfer.getData("text/plain");
    const urls = extractUrls(text.replace(/^#.*$/gm, ""));
    if (urls.length) openAdd(urls.join("\n"));
    else toast("info", t("app.noLink"));
  };

  return (
    <div className="flex h-full" onDragOver={onDragOver} onDragLeave={() => setDragging(false)} onDrop={onDrop}>
      <Sidebar />
      <main className="flex min-w-0 flex-1 flex-col">
        <Toolbar ref={searchRef} selection={selection} onNew={() => openAdd()} onSettings={() => setSettingsOpen(true)} onRemove={setRemoveIds} />
        {!isTauri() && (
          <div className="border-b border-warn/40 bg-warn/10 px-4 py-2 text-xs text-warn">
            {t("app.browserOnly")}
          </div>
        )}
        <StatsBar />
        <div className="flex min-h-0 flex-1 flex-col p-3">
          <DownloadTable rows={rows} onOpen={openFile} onRemove={setRemoveIds} onNew={() => openAdd()} />
        </div>
      </main>

      {dragging && (
        <div className="pointer-events-none fixed inset-3 z-30 flex items-center justify-center rounded-xl border-2 border-dashed border-accent bg-accent-soft/70 text-[15px] font-medium text-accent animate-fade-in">
          {t("app.dropHere")}
        </div>
      )}

      <AddDownloadDialog open={addOpen} initialUrl={addUrl} onClose={() => setAddOpen(false)} />
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
      <CaptureDialog />

      <RemoveDialog ids={removeIds} onClose={() => setRemoveIds(null)} />

      <Dialog
        open={confirmExec !== null}
        onOpenChange={(o) => !o && setConfirmExec(null)}
        title={t("app.exec.title")}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmExec(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={async () => {
                if (confirmExec) await actions.openFile(confirmExec.id, true);
                setConfirmExec(null);
              }}
            >
              {t("app.exec.run")}
            </Button>
          </>
        }
      >
        <p className="text-[13px]">
          {translateNodes(lang, "app.exec.body", { name: <strong>{confirmExec?.file_name}</strong>, run: <strong>{t("app.exec.bodyRun")}</strong> })}
        </p>
        <p className="mt-2 truncate text-xs text-muted">{t("app.exec.source", { domain: confirmExec?.domain ?? "" })}</p>
      </Dialog>
      <Toasts />
    </div>
  );
}

function RemoveDialog({ ids, onClose }: { ids: string[] | null; onClose: () => void }) {
  const t = useT();
  const [deleteFiles, setDeleteFiles] = useState(false);
  useEffect(() => setDeleteFiles(false), [ids]);
  const count = ids?.length ?? 0;
  return (
    <Dialog
      open={ids !== null}
      onOpenChange={(o) => !o && onClose()}
      title={t("app.remove.title", { count })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            onClick={async () => {
              if (ids) await actions.remove(ids, deleteFiles);
              onClose();
            }}
          >
            {t("common.remove")}
          </Button>
        </>
      }
    >
      <p className="text-[13px]">{t("app.remove.body")}</p>
      <label className={clsx("mt-3 flex items-center gap-2 text-[13px]")}>
        <input type="checkbox" checked={deleteFiles} onChange={(e) => setDeleteFiles(e.target.checked)} className="h-4 w-4 accent-[var(--odm-err)]" />
        {t("app.remove.deleteFiles")}
      </label>
    </Dialog>
  );
}
