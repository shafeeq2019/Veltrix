// Pure capture rules. Decides whether a download the browser started should
// be offered to Veltrix. No browser APIs here, so it is unit
// testable.

export interface CaptureConfig {
  enabled: boolean;
  auto_start: boolean;
  file_types: string[];
  always_take_over: string[];
  excluded_domains: string[];
  excluded_file_types: string[];
  excluded_url_patterns: string[];
  min_size_bytes: number;
  capture_private: boolean;
  send_referrer: boolean;
}

export const DEFAULT_CONFIG: CaptureConfig = {
  enabled: true,
  auto_start: false,
  file_types: [
    "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "iso", "img", "exe", "msi", "msix", "dmg", "pkg",
    "deb", "rpm", "appimage", "apk", "pdf", "epub", "mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "mp3",
    "flac", "wav", "aac", "m4a", "ogg", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "bin", "torrent",
  ],
  always_take_over: [],
  excluded_domains: [],
  excluded_file_types: [],
  excluded_url_patterns: [],
  min_size_bytes: 0,
  capture_private: false,
  send_referrer: true,
};

export interface DownloadInfo {
  url: string;
  finalUrl?: string;
  filename?: string;
  mime?: string;
  /** Size in bytes, -1 or undefined when unknown. */
  size?: number;
  incognito?: boolean;
}

export type Verdict = { capture: true } | { capture: false; reason: string };

// Streaming manifests and page resources are never handed over.
const NEVER = new Set(["m3u8", "mpd", "html", "htm", "php", "asp", "aspx", "js", "css", "json"]);

const MIME_EXT: Record<string, string> = {
  "application/zip": "zip",
  "application/x-zip-compressed": "zip",
  "application/x-7z-compressed": "7z",
  "application/vnd.rar": "rar",
  "application/x-rar-compressed": "rar",
  "application/gzip": "gz",
  "application/x-gzip": "gz",
  "application/x-tar": "tar",
  "application/pdf": "pdf",
  "application/x-msdownload": "exe",
  "application/vnd.microsoft.portable-executable": "exe",
  "application/x-msi": "msi",
  "application/x-apple-diskimage": "dmg",
  "application/x-iso9660-image": "iso",
  "video/mp4": "mp4",
  "video/webm": "webm",
  "video/x-matroska": "mkv",
  "audio/mpeg": "mp3",
  "audio/flac": "flac",
};

/** Basename of a path that may use / or \ separators. */
export function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? "";
}

export function extensionOf(name: string): string | null {
  const base = basename(name);
  const i = base.lastIndexOf(".");
  if (i <= 0 || i === base.length - 1) return null;
  const ext = base.slice(i + 1).toLowerCase();
  return ext.length <= 10 && /^[a-z0-9]+$/.test(ext) ? ext : null;
}

function urlExtension(url: string): string | null {
  try {
    const u = new URL(url);
    return extensionOf(decodeURIComponent(u.pathname));
  } catch {
    return null;
  }
}

export function domainMatches(host: string, pattern: string): boolean {
  const h = host.toLowerCase().replace(/\.$/, "");
  const p = pattern.trim().toLowerCase().replace(/\.$/, "");
  if (!p) return false;
  if (p.startsWith("*.")) return h.endsWith(p.slice(1));
  return h === p || h.endsWith(`.${p}`);
}

export function shouldCapture(d: DownloadInfo, cfg: CaptureConfig): Verdict {
  if (!cfg.enabled) return { capture: false, reason: "disabled" };
  const url = d.finalUrl || d.url;
  let host: string;
  try {
    const u = new URL(url);
    if (u.protocol !== "http:" && u.protocol !== "https:") return { capture: false, reason: "scheme" };
    host = u.hostname;
  } catch {
    return { capture: false, reason: "invalid_url" };
  }
  if (d.incognito && !cfg.capture_private) return { capture: false, reason: "private" };
  if (cfg.excluded_url_patterns.some((p) => p && (d.url.includes(p) || url.includes(p)))) {
    return { capture: false, reason: "excluded_url" };
  }
  if (cfg.excluded_domains.some((p) => domainMatches(host, p))) return { capture: false, reason: "excluded_domain" };

  const mime = (d.mime ?? "").split(";")[0].trim().toLowerCase();
  const ext = (d.filename && extensionOf(d.filename)) || urlExtension(url) || MIME_EXT[mime] || null;
  if (ext && NEVER.has(ext)) return { capture: false, reason: "never" };
  if (ext && cfg.excluded_file_types.includes(ext)) return { capture: false, reason: "excluded_type" };
  if (d.size !== undefined && d.size >= 0 && d.size < cfg.min_size_bytes) return { capture: false, reason: "too_small" };
  if (mime.startsWith("text/html") || mime === "application/xhtml+xml") return { capture: false, reason: "html" };

  if (ext && (cfg.file_types.includes(ext) || cfg.always_take_over.includes(ext))) return { capture: true };
  // Unknown type: generic binary downloads above 1 MiB are still offered.
  if (!ext && (mime === "application/octet-stream" || mime === "application/force-download") && (d.size ?? 0) > 1024 * 1024) {
    return { capture: true };
  }
  return { capture: false, reason: "type_not_listed" };
}
