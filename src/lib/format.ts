// Formatting helpers. Units are binary (1 KB = 1024 B), matching what
// download managers traditionally show.

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number | null | undefined, digits = 1): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return "–";
  if (bytes < 1024) return `${bytes} B`;
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : digits).replace(".", ",")} ${UNITS[i]}`;
}

export function formatSpeed(bps: number | null | undefined): string {
  if (!bps || bps <= 0) return "–";
  return `${formatBytes(bps)}/s`;
}

export function formatDuration(secs: number | null | undefined): string {
  if (secs === null || secs === undefined || !Number.isFinite(secs) || secs < 0) return "–";
  const s = Math.round(secs);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${s % 60} s`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h ${m % 60} min`;
  const d = Math.floor(h / 24);
  return `${d} d ${h % 24} h`;
}

export function percent(done: number, total: number | null | undefined): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.max(0, (done / total) * 100));
}

export type SpeedUnit = "KB/s" | "MB/s";

/** Convert a user entered value in KB/s or MB/s to bytes per second. */
export function toBytesPerSecond(value: number, unit: SpeedUnit): number {
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.round(value * (unit === "MB/s" ? 1024 * 1024 : 1024));
}

/** Pick a readable unit for a limit in bytes per second. */
export function fromBytesPerSecond(bps: number): { value: number; unit: SpeedUnit } {
  if (bps >= 1024 * 1024 && bps % (1024 * 1024) === 0) return { value: bps / (1024 * 1024), unit: "MB/s" };
  if (bps >= 1024 * 1024) return { value: Math.round((bps / (1024 * 1024)) * 100) / 100, unit: "MB/s" };
  return { value: Math.round(bps / 1024), unit: "KB/s" };
}

export function formatDate(ms: number | null | undefined): string {
  if (!ms) return "–";
  return new Date(ms).toLocaleString("de-DE", { dateStyle: "short", timeStyle: "short" });
}

export function extensionOf(name: string): string {
  const i = name.lastIndexOf(".");
  if (i <= 0 || i === name.length - 1) return "";
  return name.slice(i + 1).toLowerCase();
}

/** Extract http(s) URLs from free text (clipboard, drag and drop). */
export function extractUrls(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(/[\s<>"']+/)) {
    if (!/^https?:\/\//i.test(raw)) continue;
    try {
      const u = new URL(raw);
      if (u.protocol === "http:" || u.protocol === "https:") out.push(u.toString());
    } catch {
      /* ignore */
    }
  }
  return Array.from(new Set(out));
}
