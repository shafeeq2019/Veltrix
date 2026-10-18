import type { CustomCategory, DownloadStatus, DownloadView } from "./types";

export type CategoryKey =
  | "all"
  | "active"
  | "completed"
  | "queue"
  | "failed"
  | "video"
  | "music"
  | "documents"
  | "programs"
  | "archives"
  | `custom:${string}`;

export const STATUS_CATEGORIES: { key: CategoryKey; label: string }[] = [
  { key: "all", label: "Alle Downloads" },
  { key: "active", label: "In Bearbeitung" },
  { key: "completed", label: "Abgeschlossen" },
  { key: "queue", label: "Warteschlange" },
  { key: "failed", label: "Fehlgeschlagen" },
];

export const TYPE_CATEGORIES: { key: CategoryKey; label: string }[] = [
  { key: "video", label: "Video" },
  { key: "music", label: "Musik" },
  { key: "documents", label: "Dokumente" },
  { key: "programs", label: "Programme" },
  { key: "archives", label: "Archive" },
];

const ACTIVE: DownloadStatus[] = ["waiting", "connecting", "downloading", "paused"];

export function matchesCategory(d: DownloadView, key: CategoryKey): boolean {
  switch (key) {
    case "all":
      return true;
    case "active":
      return ACTIVE.includes(d.status);
    case "completed":
      return d.status === "completed";
    case "queue":
      return d.in_queue;
    case "failed":
      return d.status === "failed" || d.status === "canceled";
    default:
      if (key.startsWith("custom:")) return d.category === key.slice(7);
      return d.category === key;
  }
}

export function matchesSearch(d: DownloadView, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return (
    d.file_name.toLowerCase().includes(q) ||
    (d.domain ?? "").toLowerCase().includes(q) ||
    d.url.toLowerCase().includes(q)
  );
}

export type SortKey = "name" | "size" | "progress" | "speed" | "status" | "added" | "queue";
export interface Sort {
  key: SortKey;
  dir: "asc" | "desc";
}

const STATUS_ORDER: Record<DownloadStatus, number> = {
  downloading: 0,
  connecting: 1,
  waiting: 2,
  paused: 3,
  failed: 4,
  canceled: 5,
  completed: 6,
};

function progressOf(d: DownloadView): number {
  if (d.status === "completed") return 1;
  return d.total_size ? d.downloaded / d.total_size : 0;
}

export function compareDownloads(a: DownloadView, b: DownloadView, sort: Sort): number {
  let r = 0;
  switch (sort.key) {
    case "name":
      r = a.file_name.localeCompare(b.file_name, "de", { numeric: true, sensitivity: "base" });
      break;
    case "size":
      r = (a.total_size ?? -1) - (b.total_size ?? -1);
      break;
    case "progress":
      r = progressOf(a) - progressOf(b);
      break;
    case "speed":
      r = a.speed - b.speed;
      break;
    case "status":
      r = STATUS_ORDER[a.status] - STATUS_ORDER[b.status];
      break;
    case "queue":
      r = a.queue_position - b.queue_position;
      break;
    case "added":
      r = a.created_at - b.created_at;
      break;
  }
  if (r === 0) r = a.created_at - b.created_at;
  return sort.dir === "asc" ? r : -r;
}

export function visibleDownloads(
  all: DownloadView[],
  category: CategoryKey,
  search: string,
  sort: Sort,
): DownloadView[] {
  return all.filter((d) => matchesCategory(d, category) && matchesSearch(d, search)).sort((a, b) => compareDownloads(a, b, sort));
}

export function categoryCounts(all: DownloadView[], custom: CustomCategory[]): Record<string, number> {
  const keys: CategoryKey[] = [
    ...STATUS_CATEGORIES.map((c) => c.key),
    ...TYPE_CATEGORIES.map((c) => c.key),
    ...custom.map((c) => `custom:${c.id}` as CategoryKey),
  ];
  const out: Record<string, number> = {};
  for (const k of keys) out[k] = all.filter((d) => matchesCategory(d, k)).length;
  return out;
}
