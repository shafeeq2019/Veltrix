import type { DownloadStatus, DownloadView, ErrorCode } from "./types";

export const STATUS_LABEL: Record<DownloadStatus, string> = {
  waiting: "In Warteschlange",
  connecting: "Verbinde…",
  downloading: "Lädt herunter",
  paused: "Pausiert",
  completed: "Abgeschlossen",
  failed: "Fehlgeschlagen",
  canceled: "Abgebrochen",
};

const ERROR_HINT: Partial<Record<ErrorCode, string>> = {
  unauthorized: "Anmeldung erforderlich",
  forbidden: "Zugriff verweigert – Link evtl. abgelaufen",
  not_found: "Datei nicht gefunden",
  link_expired: "Link abgelaufen",
  rate_limited: "Server drosselt Anfragen",
  server_error: "Serverfehler",
  network: "Netzwerkfehler",
  timeout: "Zeitüberschreitung",
  tls: "Zertifikatsfehler",
  disk_full: "Datenträger voll",
  permission_denied: "Keine Schreibrechte",
  checksum_mismatch: "Prüfsumme stimmt nicht",
  size_mismatch: "Datei unvollständig",
  insecure_redirect: "Unsichere Weiterleitung blockiert",
  too_many_redirects: "Zu viele Weiterleitungen",
};

export function statusText(d: DownloadView): string {
  if (d.status === "failed" && d.error) return ERROR_HINT[d.error.code] ?? STATUS_LABEL.failed;
  if (d.status === "downloading" && d.connections_active > 1) return `Lädt (${d.connections_active} Verbindungen)`;
  return STATUS_LABEL[d.status];
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
