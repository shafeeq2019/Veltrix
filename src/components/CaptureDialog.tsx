import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Globe } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../lib/api";
import { formatBytes, formatDuration } from "../lib/format";
import { useStore } from "../lib/store";
import type { OfferResolution, OfferView } from "../lib/types";
import { FileIcon } from "./FileIcon";
import { Button, Dialog, Field, Input } from "./ui";

/** Dialog for a download handed over by the browser extension. */
export function CaptureDialog() {
  const offers = useStore((s) => s.offers);
  const dropOffer = useStore((s) => s.dropOffer);
  const toast = useStore((s) => s.toast);
  const offer: OfferView | undefined = offers[0];
  const [fileName, setFileName] = useState("");
  const [saveDir, setSaveDir] = useState("");
  const [always, setAlways] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!offer) return;
    setFileName(offer.file_name);
    setSaveDir(offer.save_dir);
    setAlways(false);
  }, [offer]);

  if (!offer) return null;

  const resolve = async (action: OfferResolution["action"]) => {
    setBusy(true);
    try {
      await api.resolveOffer(offer.offer_id, {
        action,
        always_for_type: action !== "ignore" && always,
        file_name: fileName || null,
        save_dir: saveDir || null,
      });
    } catch (e) {
      toast("error", errorMessage(e));
    } finally {
      dropOffer(offer.offer_id);
      setBusy(false);
    }
  };

  const chooseDir = async () => {
    const dir = await openDialog({ directory: true, defaultPath: saveDir || undefined, title: "Zielordner wählen" });
    if (typeof dir === "string") setSaveDir(dir);
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && resolve("ignore")}
      title="Download übernehmen?"
      description={offer.source === "context_menu" ? "Aus dem Browser-Kontextmenü gesendet" : "Im Browser gestarteter Download wurde erkannt"}
      width="max-w-xl"
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={() => resolve("ignore")} title="Der Browser lädt die Datei selbst herunter">
            Ignorieren
          </Button>
          <Button disabled={busy} onClick={() => resolve("queue")}>
            Zur Warteschlange hinzufügen
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => resolve("now")}>
            Jetzt herunterladen
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="flex items-center gap-3">
          <FileIcon category={offer.category} size={32} />
          <Input value={fileName} onChange={(e) => setFileName(e.target.value)} aria-label="Dateiname" />
        </div>
        <dl className="grid grid-cols-[120px_1fr] gap-x-3 gap-y-1.5 rounded-lg border border-line bg-panel-2 p-3.5 text-xs">
          <dt className="text-muted">Größe</dt>
          <dd className="tabular">{offer.size != null ? formatBytes(offer.size) : "unbekannt"}</dd>
          <dt className="text-muted">Domain</dt>
          <dd className="flex items-center gap-1">
            <Globe size={12} className="text-muted" /> {offer.domain ?? "–"}
          </dd>
          <dt className="text-muted">URL</dt>
          <dd className="truncate font-mono" title={offer.url}>
            {offer.url}
          </dd>
          <dt className="text-muted">Geschätzte Zeit</dt>
          <dd className="tabular">{offer.estimated_secs != null ? formatDuration(offer.estimated_secs) : "noch nicht berechenbar"}</dd>
          {offer.has_cookies && (
            <>
              <dt className="text-muted">Sitzung</dt>
              <dd>Cookies dieser Seite werden nur für diesen Download verwendet und nicht gespeichert.</dd>
            </>
          )}
        </dl>
        <Field label="Zielordner">
          <div className="flex gap-2">
            <Input value={saveDir} onChange={(e) => setSaveDir(e.target.value)} />
            <Button onClick={chooseDir}>
              <FolderOpen size={14} /> Ändern
            </Button>
          </div>
        </Field>
        {offer.extension && (
          <label className="flex items-center gap-2 text-[13px]">
            <input type="checkbox" checked={always} onChange={(e) => setAlways(e.target.checked)} className="h-4 w-4 accent-[var(--odm-accent)]" />
            Immer für .{offer.extension}-Dateien übernehmen (ohne Nachfrage)
          </label>
        )}
        <p className="text-xs text-muted">
          Die Datei wird nur gespeichert, nicht geöffnet oder ausgeführt. {offers.length > 1 && `Weitere ${offers.length - 1} Download(s) warten.`}
        </p>
      </div>
    </Dialog>
  );
}
