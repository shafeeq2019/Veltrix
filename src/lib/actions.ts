// User actions with consistent error reporting.

import { api, errorCode, errorMessage } from "./api";
import { useStore } from "./store";
import type { DownloadView } from "./types";

async function run(label: string, fn: () => Promise<unknown>): Promise<boolean> {
  try {
    await fn();
    return true;
  } catch (e) {
    useStore.getState().toast("error", `${label}: ${errorMessage(e)}`);
    return false;
  }
}

export const actions = {
  pause: (ids: string[]) => run("Pausieren fehlgeschlagen", () => api.pause(ids)),
  resume: (ids: string[]) => run("Fortsetzen fehlgeschlagen", () => api.resume(ids)),
  cancel: (ids: string[]) => run("Abbrechen fehlgeschlagen", () => api.cancel(ids)),
  retry: (ids: string[]) => run("Erneuter Versuch fehlgeschlagen", () => api.retry(ids)),
  remove: (ids: string[], deleteFiles: boolean) => run("Entfernen fehlgeschlagen", () => api.remove(ids, deleteFiles)),
  showInFolder: (id: string) => run("Ordner konnte nicht geöffnet werden", () => api.showInFolder(id)),
  addToQueue: (ids: string[]) => run("Warteschlange", () => api.setInQueue(ids, true)),
  removeFromQueue: (ids: string[]) => run("Warteschlange", () => api.setInQueue(ids, false)),
  moveInQueue: (id: string, delta: number) => run("Verschieben fehlgeschlagen", () => api.moveInQueue(id, delta)),
  setPriority: (ids: string[], p: "low" | "normal" | "high") => run("Priorität", () => api.setPriority(ids, p)),

  /** Opens a completed file. Returns "confirm" when the file is a program
   * and the user still has to confirm that it will be executed. */
  async openFile(id: string, confirmed = false): Promise<"ok" | "confirm" | "error"> {
    try {
      await api.openFile(id, confirmed);
      return "ok";
    } catch (e) {
      if (errorCode(e) === "confirmation_required") return "confirm";
      useStore.getState().toast("error", `Datei konnte nicht geöffnet werden: ${errorMessage(e)}`);
      return "error";
    }
  },

  async copyUrl(d: DownloadView) {
    try {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(d.url);
      useStore.getState().toast("success", "Download-URL kopiert");
    } catch (e) {
      useStore.getState().toast("error", `Kopieren fehlgeschlagen: ${errorMessage(e)}`);
    }
  },

  /** Toggle between pause and resume for a mixed selection. */
  toggle(ds: DownloadView[]) {
    const running = ds.filter((d) => d.status === "downloading" || d.status === "connecting" || d.status === "waiting");
    if (running.length > 0) return actions.pause(running.map((d) => d.id));
    const resumable = ds.filter((d) => d.status === "paused" || d.status === "failed" || d.status === "canceled");
    if (resumable.length > 0) return actions.resume(resumable.map((d) => d.id));
    return Promise.resolve(true);
  },
};

export function canPause(d: DownloadView) {
  return d.status === "downloading" || d.status === "connecting" || d.status === "waiting";
}
export function canResume(d: DownloadView) {
  return d.status === "paused" || d.status === "failed" || d.status === "canceled" || d.status === "waiting";
}
export function canCancel(d: DownloadView) {
  return d.status !== "completed" && d.status !== "canceled";
}
