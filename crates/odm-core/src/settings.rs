//! Persistent application settings.

use crate::category::CustomCategory;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeedMode {
    /// No artificial limit; connection counts from the settings are used.
    #[default]
    Maximum,
    /// Fewer connections, fewer parallel downloads and a soft cap of 80 % of
    /// the highest throughput observed in this session, to leave headroom
    /// for other applications.
    Balanced,
    /// Hard global limit in bytes per second.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct QueueSchedule {
    pub enabled: bool,
    /// "HH:MM", local time.
    pub start_time: String,
    /// Optional "HH:MM" at which queue downloads are paused again.
    pub stop_time: Option<String>,
    /// 0 = Monday … 6 = Sunday. Empty means every day.
    pub days: Vec<u8>,
}

impl Default for QueueSchedule {
    fn default() -> Self {
        Self { enabled: false, start_time: "02:00".into(), stop_time: Some("07:00".into()), days: vec![] }
    }
}

/// Settings that control which browser downloads the extension hands over.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CaptureSettings {
    pub enabled: bool,
    /// Start captured downloads without asking.
    pub auto_start: bool,
    /// Extensions (lower case, without dot) that are captured.
    pub file_types: Vec<String>,
    /// Extensions for which the user chose "always take over".
    pub always_take_over: Vec<String>,
    pub excluded_domains: Vec<String>,
    pub excluded_file_types: Vec<String>,
    /// Substrings; a URL containing one of them is never captured.
    pub excluded_url_patterns: Vec<String>,
    /// Do not capture files smaller than this (bytes, when known).
    pub min_size_bytes: u64,
    /// Capture downloads from private/incognito windows.
    pub capture_private: bool,
    /// Forward the Referer of captured downloads (many servers require it).
    pub send_referrer: bool,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        let types = [
            "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "iso", "img", "exe", "msi", "msix", "dmg", "pkg",
            "deb", "rpm", "appimage", "apk", "pdf", "epub", "mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "mp3",
            "flac", "wav", "aac", "m4a", "ogg", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "bin", "torrent",
        ];
        Self {
            enabled: true,
            auto_start: false,
            file_types: types.iter().map(|s| s.to_string()).collect(),
            always_take_over: vec![],
            excluded_domains: vec![],
            excluded_file_types: vec![],
            excluded_url_patterns: vec![],
            min_size_bytes: 0,
            capture_private: false,
            send_referrer: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub download_dir: PathBuf,
    /// Parallel downloads run by the queue.
    pub max_concurrent_downloads: u32,
    /// Maximum connections (segments) per download.
    pub segments_per_download: u32,
    /// Hard cap for all open download connections.
    pub max_total_connections: u32,
    /// Segments are never smaller than this (bytes).
    pub min_segment_size: u64,
    pub speed_mode: SpeedMode,
    /// Limit for [`SpeedMode::Custom`], bytes/s.
    pub speed_limit_bps: u64,
    /// Retries per connection for temporary errors.
    pub max_retries: u32,
    pub connect_timeout_secs: u64,
    pub read_timeout_secs: u64,
    /// Re-queue downloads that failed with a temporary error.
    pub auto_retry_failed: bool,
    pub auto_retry_delay_secs: u64,
    pub auto_retry_max: u32,
    pub allow_insecure_redirects: bool,
    pub resume_on_startup: bool,
    pub start_queue_on_startup: bool,
    pub remove_completed_from_list: bool,
    pub notify_on_complete: bool,
    pub notify_on_error: bool,
    pub minimize_to_tray: bool,
    pub theme: Theme,
    pub language: String,
    pub schedule: QueueSchedule,
    pub capture: CaptureSettings,
    pub custom_categories: Vec<CustomCategory>,
    /// Optional user agent override. Empty = default.
    pub user_agent: String,
    /// Chromium extension ids allowed to use the native messaging host.
    pub chromium_extension_ids: Vec<String>,
}

/// Id of the bundled extension when loaded unpacked (pinned via the `key`
/// in its manifest). Store builds get a different id that must be added.
pub const DEFAULT_CHROMIUM_EXTENSION_ID: &str = "pjmfnhocabgijmcbdcbkfeaogjnhffea";
pub const FIREFOX_EXTENSION_ID: &str = "odm@opendownload-manager.local";

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir(),
            max_concurrent_downloads: 3,
            segments_per_download: 8,
            max_total_connections: 32,
            min_segment_size: 1024 * 1024,
            speed_mode: SpeedMode::Maximum,
            speed_limit_bps: 5 * 1024 * 1024,
            max_retries: 8,
            connect_timeout_secs: 20,
            read_timeout_secs: 30,
            auto_retry_failed: true,
            auto_retry_delay_secs: 60,
            auto_retry_max: 3,
            allow_insecure_redirects: false,
            resume_on_startup: false,
            start_queue_on_startup: false,
            remove_completed_from_list: false,
            notify_on_complete: true,
            notify_on_error: true,
            minimize_to_tray: true,
            theme: Theme::System,
            language: "de".into(),
            schedule: QueueSchedule::default(),
            capture: CaptureSettings::default(),
            custom_categories: vec![],
            user_agent: String::new(),
            chromium_extension_ids: vec![DEFAULT_CHROMIUM_EXTENSION_ID.into()],
        }
    }
}

pub fn default_download_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    home.join("Downloads")
}

/// Values derived from the settings and the speed mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveLimits {
    pub max_concurrent: u32,
    pub segments: u32,
    pub total_connections: u32,
}

impl Settings {
    /// Clamp values into sane ranges. Called on every load and update so a
    /// tampered settings row cannot make the engine open thousands of sockets.
    pub fn normalize(&mut self) {
        self.max_concurrent_downloads = self.max_concurrent_downloads.clamp(1, 20);
        self.segments_per_download = self.segments_per_download.clamp(1, 32);
        self.max_total_connections = self.max_total_connections.clamp(1, 128);
        self.min_segment_size = self.min_segment_size.clamp(64 * 1024, 1024 * 1024 * 1024);
        self.max_retries = self.max_retries.min(50);
        self.connect_timeout_secs = self.connect_timeout_secs.clamp(3, 300);
        self.read_timeout_secs = self.read_timeout_secs.clamp(5, 600);
        self.auto_retry_delay_secs = self.auto_retry_delay_secs.clamp(5, 24 * 3600);
        self.auto_retry_max = self.auto_retry_max.min(100);
        if self.speed_limit_bps < 1024 {
            self.speed_limit_bps = 1024;
        }
        for list in
            [&mut self.capture.file_types, &mut self.capture.always_take_over, &mut self.capture.excluded_file_types]
        {
            for e in list.iter_mut() {
                *e = e.trim().trim_start_matches('.').to_ascii_lowercase();
            }
            list.retain(|e| !e.is_empty());
            list.sort();
            list.dedup();
        }
        self.capture.excluded_domains.iter_mut().for_each(|d| *d = d.trim().to_ascii_lowercase());
        self.capture.excluded_domains.retain(|d| !d.is_empty());
        self.capture.excluded_url_patterns.retain(|p| !p.trim().is_empty());
        self.user_agent = self.user_agent.chars().filter(|c| !c.is_control()).take(512).collect();
        // Chromium ids are exactly 32 letters a-p.
        self.chromium_extension_ids.retain(|id| id.len() == 32 && id.chars().all(|c| ('a'..='p').contains(&c)));
        self.chromium_extension_ids.dedup();
    }

    pub fn effective_limits(&self) -> EffectiveLimits {
        match self.speed_mode {
            SpeedMode::Balanced => EffectiveLimits {
                max_concurrent: self.max_concurrent_downloads.min(2),
                segments: self.segments_per_download.min(4),
                total_connections: self.max_total_connections.min(8),
            },
            _ => EffectiveLimits {
                max_concurrent: self.max_concurrent_downloads,
                segments: self.segments_per_download,
                total_connections: self.max_total_connections,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_clamps() {
        let mut s = Settings { segments_per_download: 1000, max_concurrent_downloads: 0, ..Default::default() };
        s.capture.file_types = vec![".ZIP".into(), "zip".into(), " ".into()];
        s.normalize();
        assert_eq!(s.segments_per_download, 32);
        assert_eq!(s.max_concurrent_downloads, 1);
        assert_eq!(s.capture.file_types, vec!["zip".to_string()]);
    }

    #[test]
    fn balanced_limits() {
        let s = Settings { speed_mode: SpeedMode::Balanced, ..Default::default() };
        let l = s.effective_limits();
        assert_eq!(l.segments, 4);
        assert_eq!(l.max_concurrent, 2);
    }

    #[test]
    fn deserializes_partial_json() {
        let s: Settings = serde_json::from_str(r#"{"segments_per_download": 4}"#).unwrap();
        assert_eq!(s.segments_per_download, 4);
        assert_eq!(s.max_concurrent_downloads, 3);
    }
}
