import { errorKey, statusKey, type I18n } from "../i18n";
import type { DownloadStatus, DownloadView } from "./types";

export function statusText(d: DownloadView, t: I18n["t"]): string {
  if (d.status === "failed" && d.error) return t(errorKey(d.error.code));
  if (d.status === "downloading" && d.connections_active > 1) return t("status.downloadingConnections", { count: d.connections_active });
  return t(statusKey(d.status));
}

export function statusTone(status: DownloadStatus): string {
  switch (status) {
    case "downloading":
    case "connecting":
      return "text-accent";
    case "completed":
      return "text-ok";
    case "failed":
      return "text-err";
    case "paused":
    case "canceled":
      return "text-warn";
    default:
      return "text-muted";
  }
}
