//! Typed error codes shared by the engine, the desktop UI and the IPC layer.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Stable, machine readable error codes. The frontend maps them to
/// user-facing messages, so variants must never be renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidUrl,
    UnsupportedScheme,
    InvalidPath,
    Network,
    Timeout,
    Tls,
    HttpStatus,
    Unauthorized,
    Forbidden,
    NotFound,
    LinkExpired,
    RateLimited,
    ServerError,
    TooManyRedirects,
    InsecureRedirect,
    ResourceChanged,
    RangeNotSatisfiable,
    SizeMismatch,
    ChecksumMismatch,
    DiskFull,
    PermissionDenied,
    Io,
    Storage,
    NotFoundLocal,
    InvalidState,
    Duplicate,
    ConfirmationRequired,
    Internal,
}

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, PartialEq)]
#[error("{message}")]
pub struct DownloadError {
    pub code: ErrorCode,
    pub message: String,
    /// HTTP status code, if the error originated from a server response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    /// Whether retrying later has a realistic chance of success.
    pub retryable: bool,
    /// Server requested back-off (Retry-After), not serialized.
    #[serde(skip)]
    pub retry_after: Option<Duration>,
}

impl DownloadError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let retryable =
            matches!(code, ErrorCode::Network | ErrorCode::Timeout | ErrorCode::RateLimited | ErrorCode::ServerError);
        Self { code, message: message.into(), http_status: None, retryable, retry_after: None }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// Map an HTTP status that is not acceptable for a download request.
    pub fn from_http_status(status: u16, retry_after: Option<Duration>) -> Self {
        let (code, msg) = match status {
            401 | 407 => (ErrorCode::Unauthorized, "The server requires authentication"),
            403 => (ErrorCode::Forbidden, "Access denied by the server (the link may have expired)"),
            404 => (ErrorCode::NotFound, "File not found on the server"),
            410 => (ErrorCode::LinkExpired, "The download link is no longer valid"),
            416 => (ErrorCode::RangeNotSatisfiable, "The server rejected the requested byte range"),
            408 => (ErrorCode::Timeout, "The server timed out"),
            429 => (ErrorCode::RateLimited, "Too many requests, the server asked to slow down"),
            500..=599 => (ErrorCode::ServerError, "Temporary server error"),
            _ => (ErrorCode::HttpStatus, "Unexpected server response"),
        };
        let mut e = DownloadError::new(code, format!("{msg} (HTTP {status})")).with_status(status);
        // 501 Not Implemented / 505 are not going to change by retrying.
        if matches!(status, 501 | 505) {
            e.retryable = false;
        }
        e.retry_after = retry_after;
        e
    }

    pub fn io(err: &std::io::Error, context: &str) -> Self {
        let code = match err.kind() {
            std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            std::io::ErrorKind::StorageFull => ErrorCode::DiskFull,
            std::io::ErrorKind::NotFound => ErrorCode::NotFoundLocal,
            _ if is_disk_full(err) => ErrorCode::DiskFull,
            _ => ErrorCode::Io,
        };
        DownloadError::new(code, format!("{context}: {err}")).retryable(false)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        DownloadError::new(ErrorCode::Internal, message)
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        DownloadError::new(ErrorCode::InvalidState, message)
    }
}

fn is_disk_full(err: &std::io::Error) -> bool {
    // ENOSPC on Unix (28), ERROR_DISK_FULL (112) / ERROR_HANDLE_DISK_FULL (39) on Windows.
    match err.raw_os_error() {
        #[cfg(unix)]
        Some(28) => true,
        #[cfg(windows)]
        Some(112) | Some(39) => true,
        _ => false,
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(err: reqwest::Error) -> Self {
        // reqwest errors may contain the full URL including query parameters
        // (signed tokens). Never include it in the message.
        let err = err.without_url();
        if err.is_timeout() {
            return DownloadError::new(ErrorCode::Timeout, "Connection timed out");
        }
        if err.is_redirect() {
            let text = format!("{err}");
            if text.contains("insecure redirect") {
                return DownloadError::new(
                    ErrorCode::InsecureRedirect,
                    "Refusing to follow a redirect from HTTPS to unencrypted HTTP",
                );
            }
            return DownloadError::new(ErrorCode::TooManyRedirects, "Too many redirects");
        }
        if let Some(status) = err.status() {
            return DownloadError::from_http_status(status.as_u16(), None);
        }
        let chain = error_chain(&err);
        if chain.contains("certificate") || chain.contains("tls") || chain.contains("TLS") {
            return DownloadError::new(ErrorCode::Tls, format!("TLS error: {chain}")).retryable(false);
        }
        if err.is_connect() {
            return DownloadError::new(ErrorCode::Network, format!("Connection failed: {chain}"));
        }
        if err.is_builder() {
            return DownloadError::new(ErrorCode::InvalidUrl, format!("Invalid request: {chain}"));
        }
        DownloadError::new(ErrorCode::Network, format!("Network error: {chain}"))
    }
}

fn error_chain(err: &dyn std::error::Error) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    out
}

impl From<rusqlite::Error> for DownloadError {
    fn from(err: rusqlite::Error) -> Self {
        DownloadError::new(ErrorCode::Storage, format!("Database error: {err}")).retryable(false)
    }
}

pub type Result<T, E = DownloadError> = std::result::Result<T, E>;
