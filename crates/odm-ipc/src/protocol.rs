//! Messages exchanged between the browser extension, the native messaging
//! host and the desktop app. The extension ↔ host leg uses exactly these
//! JSON shapes; the host ↔ app leg wraps them in [`AppRequest`] which adds
//! the local authentication token.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
/// Upper bound for a single message in either direction. Chrome limits
/// host → browser messages to 1 MiB, we use the same bound everywhere.
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

const MAX_URL: usize = 16 * 1024;
const MAX_NAME: usize = 1024;
const MAX_MIME: usize = 255;
const MAX_COOKIE: usize = 16 * 1024;
const MAX_ID: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope<T> {
    pub v: u32,
    /// Correlation id chosen by the sender, echoed in the response.
    pub id: String,
    #[serde(flatten)]
    pub body: T,
}

impl<T> Envelope<T> {
    pub fn new(id: impl Into<String>, body: T) -> Self {
        Envelope { v: PROTOCOL_VERSION, id: id.into(), body }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping,
    GetConfig,
    Offer(Offer),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OfferSource {
    /// A download the browser started (downloads API).
    #[default]
    BrowserDownload,
    /// "Download with OpenDownload Manager" from the context menu.
    ContextMenu,
}

/// A download the extension proposes to hand over. Contains only what is
/// needed to perform the download — no page content, no history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Offer {
    pub url: String,
    pub final_url: Option<String>,
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub size: Option<u64>,
    pub referrer: Option<String>,
    /// `Cookie` header for this URL. Only sent when the user explicitly
    /// enabled cookie forwarding in the extension.
    pub cookies: Option<String>,
    pub source: OfferSource,
    pub private: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// The app took over and started the download.
    Accepted,
    /// The app took over and queued the download.
    Queued,
    /// The user declined; the browser should continue its own download.
    Ignored,
    /// Already in the download list; the browser download can be dropped.
    Duplicate,
}

/// Capture configuration served to the extension (subset of app settings).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct CaptureConfig {
    pub enabled: bool,
    pub auto_start: bool,
    pub file_types: Vec<String>,
    pub always_take_over: Vec<String>,
    pub excluded_domains: Vec<String>,
    pub excluded_file_types: Vec<String>,
    pub excluded_url_patterns: Vec<String>,
    pub min_size_bytes: u64,
    pub capture_private: bool,
    pub send_referrer: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    BadRequest,
    Unsupported,
    Unauthorized,
    AppNotRunning,
    Timeout,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong { app_version: String },
    Config { config: CaptureConfig },
    Decision { decision: Decision },
    Error { code: IpcErrorCode, message: String },
}

impl Response {
    pub fn error(code: IpcErrorCode, message: impl Into<String>) -> Self {
        Response::Error { code, message: message.into() }
    }
}

/// Host → app request with the shared secret from the token file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRequest {
    pub token: String,
    /// Browser origin as reported to the host (e.g. `chrome-extension://…/`).
    pub origin: Option<String>,
    pub request: Envelope<Request>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ValidationError(pub String);

fn check_len(name: &str, value: &Option<String>, max: usize) -> Result<(), ValidationError> {
    match value {
        Some(v) if v.len() > max => Err(ValidationError(format!("{name} is too long"))),
        Some(v) if v.chars().any(|c| c.is_control()) => Err(ValidationError(format!("{name} contains control characters"))),
        _ => Ok(()),
    }
}

fn check_http_url(name: &str, value: &str) -> Result<(), ValidationError> {
    if value.len() > MAX_URL {
        return Err(ValidationError(format!("{name} is too long")));
    }
    let parsed = url::Url::parse(value).map_err(|_| ValidationError(format!("{name} is not a valid URL")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ValidationError(format!("{name} must use http or https")));
    }
    if parsed.host_str().is_none() {
        return Err(ValidationError(format!("{name} has no host")));
    }
    Ok(())
}

impl Envelope<Request> {
    /// Structural validation done by both host and app before acting.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.v != PROTOCOL_VERSION {
            return Err(ValidationError(format!("unsupported protocol version {}", self.v)));
        }
        if self.id.is_empty() || self.id.len() > MAX_ID || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(ValidationError("invalid message id".into()));
        }
        if let Request::Offer(o) = &self.body {
            check_http_url("url", &o.url)?;
            if let Some(f) = &o.final_url {
                check_http_url("final_url", f)?;
            }
            if let Some(r) = &o.referrer {
                check_http_url("referrer", r)?;
            }
            check_len("filename", &o.filename, MAX_NAME)?;
            check_len("mime", &o.mime, MAX_MIME)?;
            check_len("cookies", &o.cookies, MAX_COOKIE)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shapes_are_stable() {
        let req: Envelope<Request> = serde_json::from_str(
            r#"{"v":1,"id":"a1","type":"offer","url":"https://x.org/f.zip","filename":"f.zip","size":12,"source":"context_menu"}"#,
        )
        .unwrap();
        assert!(matches!(&req.body, Request::Offer(o) if o.size == Some(12) && o.source == OfferSource::ContextMenu));
        assert!(req.validate().is_ok());
        let ping: Envelope<Request> = serde_json::from_str(r#"{"v":1,"id":"p","type":"ping"}"#).unwrap();
        assert_eq!(ping.body, Request::Ping);
        let resp = serde_json::to_string(&Envelope::new("a1", Response::Decision { decision: Decision::Queued })).unwrap();
        assert_eq!(resp, r#"{"v":1,"id":"a1","type":"decision","decision":"queued"}"#);
    }

    #[test]
    fn rejects_bad_offers() {
        let mk = |o: Offer| Envelope::new("x", Request::Offer(o));
        assert!(mk(Offer { url: "javascript:alert(1)".into(), ..Default::default() }).validate().is_err());
        assert!(mk(Offer { url: "file:///etc/passwd".into(), ..Default::default() }).validate().is_err());
        assert!(mk(Offer { url: "https://a.b/".into(), filename: Some("a\nb".into()), ..Default::default() })
            .validate()
            .is_err());
        assert!(mk(Offer { url: "https://a.b/".into(), referrer: Some("chrome://x".into()), ..Default::default() })
            .validate()
            .is_err());
        assert!(mk(Offer { url: format!("https://a.b/{}", "x".repeat(MAX_URL)), ..Default::default() })
            .validate()
            .is_err());
        let mut bad_id = mk(Offer { url: "https://a.b/".into(), ..Default::default() });
        bad_id.id = "../../".into();
        assert!(bad_id.validate().is_err());
        let mut bad_v = mk(Offer { url: "https://a.b/".into(), ..Default::default() });
        bad_v.v = 99;
        assert!(bad_v.validate().is_err());
    }

    #[test]
    fn unknown_type_is_rejected() {
        assert!(serde_json::from_str::<Envelope<Request>>(r#"{"v":1,"id":"x","type":"exec","cmd":"rm"}"#).is_err());
    }
}
