import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, FolderOpen, Globe, KeyRound, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, errorCode, errorMessage } from "../lib/api";
import { extractUrls, formatBytes, formatDuration } from "../lib/format";
import { useStore } from "../lib/store";
import type { ProbeInfo, StartMode } from "../lib/types";
import { FileIcon } from "./FileIcon";
import { Button, Dialog, Field, Input } from "./ui";

interface Props {
  open: boolean;
  initialUrl: string;
  onClose: () => void;
}

export function AddDownloadDialog({ open, initialUrl, onClose }: Props) {
  const settings = useStore((s) => s.settings);
  const toast = useStore((s) => s.toast);
  const [url, setUrl] = useState(initialUrl);
  const [showAuth, setShowAuth] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [probing, setProbing] = useState(false);
  const [probe, setProbe] = useState<ProbeInfo | null>(null);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [fileName, setFileName] = useState("");
  const [saveDir, setSaveDir] = useState("");
  const [sha256, setSha256] = useState("");
  const [bandwidth, setBandwidth] = useState(0);
  const [busy, setBusy] = useState(false);
  const urlRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setUrl(initialUrl);
    setProbe(null);
    setProbeError(null);
    setFileName("");
    setSha256("");
    setSaveDir(settings?.download_dir ?? "");
    api.appInfo().then((i) => setBandwidth(i.bandwidth_estimate)).catch(() => {});
    if (!initialUrl) {
      // Prefill from the clipboard when it holds a link.
      import("@tauri-apps/plugin-clipboard-manager")
        .then((m) => m.readText())
        .then((t) => {
          const found = extractUrls(t ?? "");
          if (found.length > 0) setUrl((cur) => cur || found.join("\n"));
        })
        .catch(() => {});
    }
    setTimeout(() => urlRef.current?.focus(), 50);
  }, [open, initialUrl, settings?.download_dir]);

  const urls = extractUrls(url);
  const multiple = urls.length > 1;

  const runProbe = async () => {
    if (urls.length !== 1) return;
    setProbing(true);
    setProbeError(null);
    try {
      const info = await api.probe(urls[0], null, username || null, password || null);
      setProbe(info);
      setFileName(info.file_name);
    } catch (e) {
      setProbe(null);
      setProbeError(errorMessage(e));
      // Still allow adding: name derived from the URL.
      try {
        const last = decodeURIComponent(new URL(urls[0]).pathname.split("/").filter(Boolean).pop() ?? "");
        setFileName(last || "download");
      } catch {
        setFileName("download");
      }
    } finally {
      setProbing(false);
    }
  };

  const chooseDir = async () => {
    const dir = await openDialog({ directory: true, defaultPath: saveDir || undefined, title: "Zielordner wählen" });
    if (typeof dir === "string") setSaveDir(dir);
  };

  const submit = async (start: StartMode) => {
    setBusy(true);
    try {
      if (multiple) {
        let ok = 0;
        for (const u of urls) {
          try {
            await api.add({ url: u, start, save_dir: saveDir || null });
            ok++;
          } catch (e) {
            toast("error", `${u}: ${errorMessage(e)}`);
          }
        }
        toast("success", `${ok} von ${urls.length} Downloads hinzugefügt`);
      } else {
        await api.add({
          url: urls[0],
          file_name: fileName || null,
          save_dir: saveDir || null,
          start,
          total_size: probe?.total_size ?? null,
          mime: probe?.mime ?? null,
          expected_sha256: sha256.trim() || null,
          username: username || null,
          password: password || null,
        });
      }
      onClose();
    } catch (e) {
      if (errorCode(e) === "duplicate") toast("info", "Diese URL ist bereits in der Liste.");
      else toast("error", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const ready = multiple || probe !== null || probeError !== null;
  const eta = probe?.total_size && bandwidth > 0 ? formatDuration(probe.total_size / bandwidth) : null;

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title="Neuer Download"
      description="URL eingeben, einfügen oder einen Link in das Fenster ziehen."
      width="max-w-xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Abbrechen
          </Button>
          {!ready ? (
            <Button variant="primary" disabled={urls.length !== 1 || probing} onClick={runProbe}>
              {probing && <Loader2 size={14} className="animate-spin" />} Weiter
            </Button>
          ) : (
            <>
              <Button disabled={busy} onClick={() => submit("queue")}>
                Zur Warteschlange
              </Button>
              <Button variant="primary" disabled={busy} onClick={() => submit("now")}>
                Jetzt herunterladen
              </Button>
            </>
          )}
        </>
      }
    >
      <div className="space-y-4">
        <Field label="Download-URL(s)" hint={multiple ? `${urls.length} Links erkannt – sie werden einzeln hinzugefügt.` : undefined}>
          <textarea
            ref={urlRef as unknown as React.Ref<HTMLTextAreaElement>}
            value={url}
            rows={multiple ? 4 : 2}
            onChange={(e) => {
              setUrl(e.target.value);
              setProbe(null);
              setProbeError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !ready) {
                e.preventDefault();
                runProbe();
              }
            }}
            placeholder="https://example.com/datei.zip"
            spellCheck={false}
            className="w-full resize-none rounded-md border border-line bg-panel px-2.5 py-2 font-mono text-xs text-fg focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25"
          />
        </Field>

        <button className="flex items-center gap-1 text-xs font-medium text-muted hover:text-fg" onClick={() => setShowAuth(!showAuth)}>
          {showAuth ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          <KeyRound size={13} /> HTTP-Anmeldung
        </button>
        {showAuth && (
          <div className="grid grid-cols-2 gap-3">
            <Field label="Benutzername">
              <Input value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="off" />
            </Field>
            <Field label="Passwort" hint="Wird nur im Arbeitsspeicher gehalten.">
              <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="off" />
            </Field>
          </div>
        )}

        {probeError && (
          <div className="rounded-md border border-warn/40 bg-warn/10 px-3 py-2 text-xs text-warn">
            Dateiinformationen konnten nicht abgerufen werden: {probeError}. Der Download kann trotzdem hinzugefügt werden.
          </div>
        )}

        {!multiple && ready && (
          <div className="space-y-3 rounded-lg border border-line bg-panel-2 p-3.5">
            <div className="flex items-center gap-3">
              <FileIcon category={probe?.category ?? "other"} size={28} />
              <div className="min-w-0 flex-1">
                <Input value={fileName} onChange={(e) => setFileName(e.target.value)} aria-label="Dateiname" />
              </div>
            </div>
            <dl className="grid grid-cols-[110px_1fr] gap-x-3 gap-y-1.5 text-xs">
              <dt className="text-muted">Größe</dt>
              <dd className="tabular">{probe?.total_size != null ? formatBytes(probe.total_size) : "unbekannt"}</dd>
              <dt className="text-muted">Domain</dt>
              <dd className="flex items-center gap-1 truncate">
                <Globe size={12} className="text-muted" /> {probe?.domain ?? new URL(urls[0]).host}
              </dd>
              <dt className="text-muted">URL</dt>
              <dd className="truncate font-mono" title={probe?.final_url ?? urls[0]}>
                {probe?.final_url ?? urls[0]}
              </dd>
              <dt className="text-muted">Fortsetzbar</dt>
              <dd>{probe ? (probe.resumable ? "Ja – segmentiert & fortsetzbar" : "Nein – eine Verbindung, kein Fortsetzen") : "unbekannt"}</dd>
              <dt className="text-muted">Geschätzte Zeit</dt>
              <dd className="tabular">{eta ?? "noch nicht berechenbar"}</dd>
            </dl>
          </div>
        )}

        {ready && (
          <Field label="Zielordner">
            <div className="flex gap-2">
              <Input value={saveDir} onChange={(e) => setSaveDir(e.target.value)} />
              <Button onClick={chooseDir} title="Ordner wählen">
                <FolderOpen size={14} /> Ändern
              </Button>
            </div>
          </Field>
        )}
        {ready && !multiple && (
          <Field label="SHA-256-Prüfsumme (optional)" hint="Wenn angegeben, wird die Datei nach dem Download verifiziert.">
            <Input value={sha256} onChange={(e) => setSha256(e.target.value)} placeholder="64 Hex-Zeichen" className="font-mono text-xs" />
          </Field>
        )}
      </div>
    </Dialog>
  );
}
