// User actions with consistent error reporting.

import { errorText, i18n, type MessageKey } from "../i18n";
import { api, errorCode } from "./api";
import { useStore } from "./store";
import type { DownloadView } from "./types";

function fail(label: MessageKey, e: unknown) {
  const { t } = i18n();
  useStore.getState().toast("error", `${t(label)}: ${errorText(e, t)}`);
}

async function run(label: MessageKey, fn: () => Promise<unknown>): Promise<boolean> {
  try {
    await fn();
    return true;
  } catch (e) {
    fail(label, e);
    return false;
  }
}

export const actions = {
  pause: (ids: string[]) => run("action.pauseFailed", () => api.pause(ids)),
  resume: (ids: string[]) => run("action.resumeFailed", () => api.resume(ids)),
  cancel: (ids: string[]) => run("action.cancelFailed", () => api.cancel(ids)),
  retry: (ids: string[]) => run("action.retryFailed", () => api.retry(ids)),
  remove: (ids: string[], deleteFiles: boolean) => run("action.removeFailed", () => api.remove(ids, deleteFiles)),
  showInFolder: (id: string) => run("action.showInFolderFailed", () => api.showInFolder(id)),
  addToQueue: (ids: string[]) => run("action.queue", () => api.setInQueue(ids, true)),
  removeFromQueue: (ids: string[]) => run("action.queue", () => api.setInQueue(ids, false)),
  moveInQueue: (id: string, delta: number) => run("action.moveFailed", () => api.moveInQueue(id, delta)),
  setPriority: (ids: string[], p: "low" | "normal" | "high") => run("action.priority", () => api.setPriority(ids, p)),

  /** Opens a completed file. Returns "confirm" when the file is a program
   * and the user still has to confirm that it will be executed. */
  async openFile(id: string, confirmed = false): Promise<"ok" | "confirm" | "error"> {
    try {
      await api.openFile(id, confirmed);
      return "ok";
    } catch (e) {
      if (errorCode(e) === "confirmation_required") return "confirm";
      fail("action.openFailed", e);
      return "error";
    }
  },

  async copyUrl(d: DownloadView) {
    try {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(d.url);
      useStore.getState().toast("success", i18n().t("action.urlCopied"));
    } catch (e) {
      fail("action.copyFailed", e);
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
