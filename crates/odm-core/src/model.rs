//! Data model shared by engine, storage and UI.

use crate::error::{DownloadError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

pub type DownloadId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// Waiting in the queue.
    Waiting,
    /// Request sent, waiting for the server.
    Connecting,
    Downloading,
    Paused,
    Completed,
    Failed,
    Canceled,
}

impl DownloadStatus {
    pub fn is_active(self) -> bool {
        matches!(self, DownloadStatus::Connecting | DownloadStatus::Downloading)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DownloadStatus::Waiting => "waiting",
            DownloadStatus::Connecting => "connecting",
            DownloadStatus::Downloading => "downloading",
            DownloadStatus::Paused => "paused",
            DownloadStatus::Completed => "completed",
            DownloadStatus::Failed => "failed",
            DownloadStatus::Canceled => "canceled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "waiting" => DownloadStatus::Waiting,
            "connecting" => DownloadStatus::Connecting,
            "downloading" => DownloadStatus::Downloading,
            "paused" => DownloadStatus::Paused,
            "completed" => DownloadStatus::Completed,
            "failed" => DownloadStatus::Failed,
            "canceled" => DownloadStatus::Canceled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

impl Priority {
    pub fn as_i64(self) -> i64 {
        match self {
            Priority::Low => -1,
            Priority::Normal => 0,
            Priority::High => 1,
        }
    }

    pub fn from_i64(v: i64) -> Self {
        match v {
            i64::MIN..=-1 => Priority::Low,
            0 => Priority::Normal,
            _ => Priority::High,
        }
    }
}

/// Persisted byte range of a segmented download. `end` is inclusive; `None`
/// means "until the server closes the stream" (unknown size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentState {
    pub start: u64,
    pub end: Option<u64>,
    /// Bytes that are known to be on disk, counted from `start`.
    pub written: u64,
}

impl SegmentState {
    /// Length in bytes; `None` while the end is unknown.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> Option<u64> {
        self.end.map(|e| e + 1 - self.start)
    }

    pub fn is_complete(&self) -> bool {
        self.len().map(|l| self.written >= l).unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorInfo {
    pub code: ErrorCode,
    pub message: String,
    pub http_status: Option<u16>,
    pub retryable: bool,
}

impl From<&DownloadError> for ErrorInfo {
    fn from(e: &DownloadError) -> Self {
        ErrorInfo { code: e.code, message: e.message.clone(), http_status: e.http_status, retryable: e.retryable }
    }
}

/// A download as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Download {
    pub id: DownloadId,
    pub url: String,
    /// URL after redirects, if different.
    pub final_url: Option<String>,
    pub referrer: Option<String>,
    pub file_name: String,
    /// The user chose the name explicitly; do not replace it with the
    /// server provided name.
    pub name_locked: bool,
    pub save_dir: PathBuf,
    pub total_size: Option<u64>,
    pub downloaded: u64,
    pub status: DownloadStatus,
    pub error: Option<ErrorInfo>,
    pub category: String,
    pub priority: Priority,
    pub in_queue: bool,
    pub queue_position: i64,
    pub resumable: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub mime: Option<String>,
    pub segments: Vec<SegmentState>,
    /// Requested connections; 0 = use the global setting.
    pub connections: u32,
    pub expected_sha256: Option<String>,
    pub retry_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    /// Average speed of the last run in bytes/s.
    pub avg_speed: u64,
    /// Accumulated active download time in milliseconds.
    pub active_ms: u64,
    /// The download needs credentials or cookies that are only held in
    /// memory and were lost on restart.
    pub requires_auth: bool,
}

impl Download {
    pub fn target_path(&self) -> PathBuf {
        self.save_dir.join(&self.file_name)
    }

    pub fn part_path(&self) -> PathBuf {
        crate::filename::part_path(&self.target_path())
    }
}

/// Request-scoped secrets. They are kept in memory only and never written
/// to the database or logs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestAuth {
    pub basic: Option<crate::url_util::BasicCredentials>,
    /// `Cookie` header value forwarded explicitly by the browser extension
    /// for this single download.
    pub cookie_header: Option<String>,
}

impl RequestAuth {
    pub fn is_empty(&self) -> bool {
        self.basic.is_none() && self.cookie_header.is_none()
    }
}

/// Parameters for adding a download.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AddDownloadRequest {
    pub url: String,
    pub file_name: Option<String>,
    pub save_dir: Option<PathBuf>,
    pub referrer: Option<String>,
    pub start: StartMode,
    pub priority: Priority,
    pub connections: Option<u32>,
    pub expected_sha256: Option<String>,
    /// Size/MIME already known (e.g. from the browser or a probe).
    pub total_size: Option<u64>,
    pub mime: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Allow adding a URL that is already in the list and not finished.
    pub allow_duplicate: bool,
    #[serde(skip)]
    pub cookie_header: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StartMode {
    /// Start immediately, independent of the queue.
    #[default]
    Now,
    /// Add to the queue; it starts when the queue runs.
    Queue,
    /// Add paused.
    Paused,
}

/// What the UI renders: the stored record plus live values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DownloadView {
    #[serde(flatten)]
    pub download: Download,
    pub speed: u64,
    pub eta_secs: Option<u64>,
    pub connections_active: u32,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressItem {
    pub id: DownloadId,
    pub downloaded: u64,
    pub total_size: Option<u64>,
    pub speed: u64,
    pub avg_speed: u64,
    pub eta_secs: Option<u64>,
    pub connections_active: u32,
    pub status: DownloadStatus,
}

/// Result of [`crate::http::probe`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbeInfo {
    pub url: String,
    pub final_url: String,
    pub file_name: String,
    pub total_size: Option<u64>,
    pub mime: Option<String>,
    pub resumable: bool,
    pub domain: Option<String>,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Added { download: DownloadView },
    Updated { download: DownloadView },
    Removed { id: DownloadId },
    Progress { items: Vec<ProgressItem>, total_speed: u64 },
    Completed { id: DownloadId, file_name: String, path: PathBuf },
    Failed { id: DownloadId, file_name: String, error: ErrorInfo },
    QueueState { running: bool },
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
