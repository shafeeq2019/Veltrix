// Mirrors of the Rust types in crates/odm-core/src/model.rs and settings.rs.
// Keep field names in sync with the serde (snake_case) representation.

export type DownloadStatus =
  | "waiting"
  | "connecting"
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "canceled";

export type Priority = "low" | "normal" | "high";

export type ErrorCode =
  | "invalid_url"
  | "unsupported_scheme"
  | "invalid_path"
  | "network"
  | "timeout"
  | "tls"
  | "http_status"
  | "unauthorized"
  | "forbidden"
  | "not_found"
  | "link_expired"
  | "rate_limited"
  | "server_error"
  | "too_many_redirects"
  | "insecure_redirect"
  | "resource_changed"
  | "range_not_satisfiable"
  | "size_mismatch"
  | "checksum_mismatch"
  | "disk_full"
  | "permission_denied"
  | "io"
  | "storage"
  | "not_found_local"
  | "invalid_state"
  | "duplicate"
  | "confirmation_required"
  | "internal";

export interface ErrorInfo {
  code: ErrorCode;
  message: string;
  http_status: number | null;
  retryable: boolean;
}

export interface SegmentState {
  start: number;
  end: number | null;
  written: number;
}

export interface DownloadView {
  id: string;
  url: string;
  final_url: string | null;
  referrer: string | null;
  file_name: string;
  name_locked: boolean;
  save_dir: string;
  total_size: number | null;
  downloaded: number;
  status: DownloadStatus;
  error: ErrorInfo | null;
  category: string;
  priority: Priority;
  in_queue: boolean;
  queue_position: number;
  resumable: boolean;
  etag: string | null;
  last_modified: string | null;
  mime: string | null;
  segments: SegmentState[];
  connections: number;
  expected_sha256: string | null;
  retry_count: number;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
  avg_speed: number;
  active_ms: number;
  requires_auth: boolean;
  // live values
  speed: number;
  eta_secs: number | null;
  connections_active: number;
  domain: string | null;
}

export interface ProgressItem {
  id: string;
  downloaded: number;
  total_size: number | null;
  speed: number;
  avg_speed: number;
  eta_secs: number | null;
  connections_active: number;
  status: DownloadStatus;
}

export type EngineEvent =
  | { type: "added"; download: DownloadView }
  | { type: "updated"; download: DownloadView }
  | { type: "removed"; id: string }
  | { type: "progress"; items: ProgressItem[]; total_speed: number }
  | { type: "completed"; id: string; file_name: string; path: string }
  | { type: "failed"; id: string; file_name: string; error: ErrorInfo }
  | { type: "queue_state"; running: boolean };

export type StartMode = "now" | "queue" | "paused";

export interface AddDownloadRequest {
  url: string;
  file_name?: string | null;
  save_dir?: string | null;
  referrer?: string | null;
  start?: StartMode;
  priority?: Priority;
  connections?: number | null;
  expected_sha256?: string | null;
  total_size?: number | null;
  mime?: string | null;
  username?: string | null;
  password?: string | null;
  allow_duplicate?: boolean;
}

export interface ProbeInfo {
  url: string;
  final_url: string;
  file_name: string;
  total_size: number | null;
  mime: string | null;
  resumable: boolean;
  domain: string | null;
  category: string;
}

export type SpeedMode = "maximum" | "balanced" | "custom";
export type Theme = "system" | "light" | "dark";

export interface QueueSchedule {
  enabled: boolean;
  start_time: string;
  stop_time: string | null;
  days: number[];
}

export interface CaptureSettings {
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

export interface CustomCategory {
  id: string;
  name: string;
  extensions: string[];
}

export interface Settings {
  download_dir: string;
  max_concurrent_downloads: number;
  segments_per_download: number;
  max_total_connections: number;
  min_segment_size: number;
  speed_mode: SpeedMode;
  speed_limit_bps: number;
  max_retries: number;
  connect_timeout_secs: number;
  read_timeout_secs: number;
  auto_retry_failed: boolean;
  auto_retry_delay_secs: number;
  auto_retry_max: number;
  allow_insecure_redirects: boolean;
  resume_on_startup: boolean;
  start_queue_on_startup: boolean;
  remove_completed_from_list: boolean;
  notify_on_complete: boolean;
  notify_on_error: boolean;
  minimize_to_tray: boolean;
  theme: Theme;
  language: string;
  schedule: QueueSchedule;
  capture: CaptureSettings;
  custom_categories: CustomCategory[];
  user_agent: string;
  chromium_extension_ids: string[];
}

export interface AppInfo {
  version: string;
  data_dir: string;
  queue_running: boolean;
  bandwidth_estimate: number;
  platform: string;
}

export interface OfferView {
  offer_id: string;
  url: string;
  domain: string | null;
  file_name: string;
  size: number | null;
  mime: string | null;
  category: string;
  extension: string | null;
  save_dir: string;
  estimated_secs: number | null;
  source: "browser_download" | "context_menu";
  has_cookies: boolean;
}

export interface OfferResolution {
  action: "now" | "queue" | "ignore";
  always_for_type: boolean;
  file_name?: string | null;
  save_dir?: string | null;
}

export interface BrowserRegistration {
  browser: string;
  registered: boolean;
  location: string;
}

export interface IntegrationStatus {
  host_path: string | null;
  browsers: BrowserRegistration[];
}

export interface CmdError {
  code: ErrorCode;
  message: string;
}
