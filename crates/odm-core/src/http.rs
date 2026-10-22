//! HTTP client construction, request building and response metadata.

use crate::category;
use crate::error::{DownloadError, ErrorCode, Result};
use crate::filename;
use crate::model::{ProbeInfo, RequestAuth};
use crate::url_util;
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use std::time::Duration;

pub const DEFAULT_USER_AGENT: &str = concat!("Veltrix/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub allow_insecure_redirects: bool,
    pub user_agent: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(20),
            read_timeout: Duration::from_secs(30),
            allow_insecure_redirects: false,
            user_agent: DEFAULT_USER_AGENT.into(),
        }
    }
}

/// Build the shared download client.
///
/// * TLS certificates are verified against the operating system trust
///   store (rustls + platform verifier). There is no option to disable it.
/// * HTTP/1.1 only, so that every segment uses its own TCP connection.
/// * No transparent decompression: byte ranges must refer to the raw entity.
/// * Redirects are followed up to 10 hops; HTTPS→HTTP downgrades are refused
///   unless explicitly allowed. reqwest strips `Authorization` and `Cookie`
///   when a redirect leaves the original host.
pub fn build_client(cfg: &HttpConfig) -> Result<Client> {
    let allow_insecure = cfg.allow_insecure_redirects;
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        let downgrade = attempt.previous().last().map(|p| p.scheme() == "https").unwrap_or(false)
            && attempt.url().scheme() == "http";
        if downgrade && !allow_insecure {
            return attempt.error("insecure redirect from https to http");
        }
        if !matches!(attempt.url().scheme(), "http" | "https") {
            return attempt.error("redirect to unsupported scheme");
        }
        attempt.follow()
    });
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    let ua = if cfg.user_agent.trim().is_empty() { DEFAULT_USER_AGENT } else { cfg.user_agent.trim() };
    Client::builder()
        .user_agent(ua)
        .default_headers(headers)
        .redirect(policy)
        .connect_timeout(cfg.connect_timeout)
        .read_timeout(cfg.read_timeout)
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(16)
        .http1_only()
        .https_only(false)
        .build()
        .map_err(|e| DownloadError::internal(format!("Cannot create HTTP client: {e}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: u64,
    pub end: u64,
    pub total: Option<u64>,
}

/// Parse `Content-Range: bytes 0-99/1000` (or `/*`).
pub fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.trim().split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    if end < start {
        return None;
    }
    let total = match total.trim() {
        "*" => None,
        t => {
            let t: u64 = t.parse().ok()?;
            if end >= t {
                return None;
            }
            Some(t)
        }
    };
    Some(ContentRange { start, end, total })
}

/// `Retry-After` as seconds or HTTP date, capped at 10 minutes.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    let v = value.trim();
    let secs = if let Ok(s) = v.parse::<u64>() {
        s
    } else {
        let date = chrono::DateTime::parse_from_rfc2822(v).ok()?;
        let diff = date.timestamp() - chrono::Utc::now().timestamp();
        diff.max(0) as u64
    };
    Some(Duration::from_secs(secs.min(600)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResponseMeta {
    pub status: u16,
    pub final_url: String,
    pub content_length: Option<u64>,
    pub content_range: Option<ContentRange>,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_disposition: Option<String>,
    pub mime: Option<String>,
}

impl ResponseMeta {
    pub fn from_response(resp: &Response) -> Self {
        let h = resp.headers();
        let get = |name: header::HeaderName| h.get(name).and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string());
        let content_range = get(header::CONTENT_RANGE).and_then(|v| parse_content_range(&v));
        ResponseMeta {
            status: resp.status().as_u16(),
            final_url: resp.url().to_string(),
            content_length: h
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok()),
            content_range,
            accept_ranges: get(header::ACCEPT_RANGES).map(|v| v.eq_ignore_ascii_case("bytes")).unwrap_or(false),
            etag: get(header::ETAG),
            last_modified: get(header::LAST_MODIFIED),
            content_disposition: get(header::CONTENT_DISPOSITION),
            mime: get(header::CONTENT_TYPE),
        }
    }

    /// Size of the full entity, if it can be determined.
    pub fn total_size(&self) -> Option<u64> {
        match self.status {
            206 => self.content_range.and_then(|r| r.total),
            200 => self.content_length,
            _ => None,
        }
    }

    /// Strong validator usable in `If-Range` (weak ETags are not allowed there).
    pub fn if_range_validator(etag: Option<&str>, last_modified: Option<&str>) -> Option<String> {
        match etag {
            Some(e) if !e.starts_with("W/") && !e.is_empty() => Some(e.to_string()),
            _ => last_modified.map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RequestSpec<'a> {
    pub url: &'a str,
    pub referrer: Option<&'a str>,
    pub auth: Option<&'a RequestAuth>,
    /// Inclusive byte range; `(start, None)` means "from start to the end".
    pub range: Option<(u64, Option<u64>)>,
    pub if_range: Option<&'a str>,
}

pub fn build_request(client: &Client, spec: &RequestSpec<'_>) -> reqwest::RequestBuilder {
    let mut req = client.get(spec.url);
    if let Some((start, end)) = spec.range {
        let value = match end {
            Some(end) => format!("bytes={start}-{end}"),
            None => format!("bytes={start}-"),
        };
        req = req.header(header::RANGE, value);
        if let Some(v) = spec.if_range {
            req = req.header(header::IF_RANGE, v);
        }
    }
    if let Some(r) = spec.referrer {
        if url_util::validate_download_url(r).is_ok() {
            req = req.header(header::REFERER, r);
        }
    }
    if let Some(auth) = spec.auth {
        if let Some(b) = &auth.basic {
            req = req.basic_auth(&b.username, b.password.as_deref());
        }
        if let Some(c) = &auth.cookie_header {
            if let Ok(v) = HeaderValue::from_str(c) {
                req = req.header(header::COOKIE, v);
            }
        }
    }
    req
}

/// Send a request and accept only 200/206. Every other status becomes a
/// typed error (including `Retry-After` for 429/503).
pub async fn send(client: &Client, spec: &RequestSpec<'_>) -> Result<(Response, ResponseMeta)> {
    let resp = build_request(client, spec).send().await?;
    let status = resp.status();
    if status == StatusCode::OK || status == StatusCode::PARTIAL_CONTENT {
        let meta = ResponseMeta::from_response(&resp);
        return Ok((resp, meta));
    }
    let retry_after = resp.headers().get(header::RETRY_AFTER).and_then(|v| v.to_str().ok()).and_then(parse_retry_after);
    Err(DownloadError::from_http_status(status.as_u16(), retry_after))
}

/// Ask the server about a URL without downloading the body: an open-ended
/// range request whose body is dropped right after the headers arrive.
pub async fn probe(
    client: &Client,
    url: &str,
    referrer: Option<&str>,
    auth: Option<&RequestAuth>,
    custom: &[category::CustomCategory],
) -> Result<ProbeInfo> {
    let spec = RequestSpec { url, referrer, auth, range: Some((0, None)), if_range: None };
    let (resp, meta) = match send(client, &spec).await {
        // Empty files answer an open range with 416; retry without range.
        Err(e) if e.code == ErrorCode::RangeNotSatisfiable => {
            send(client, &RequestSpec { range: None, ..spec.clone() }).await?
        }
        other => other?,
    };
    drop(resp);
    let original = url::Url::parse(url).ok();
    let final_url = url::Url::parse(&meta.final_url).ok();
    let mime = meta.mime.clone().map(|m| m.split(';').next().unwrap_or("").trim().to_string());
    let file_name = filename::choose_file_name(
        meta.content_disposition.as_deref(),
        final_url.as_ref(),
        original.as_ref(),
        mime.as_deref(),
    );
    let resumable = meta.status == 206 && meta.content_range.and_then(|r| r.total).is_some();
    Ok(ProbeInfo {
        url: url.to_string(),
        final_url: meta.final_url.clone(),
        category: category::classify(&file_name, mime.as_deref(), custom),
        file_name,
        total_size: meta.total_size(),
        mime,
        resumable,
        domain: url_util::domain_of(&meta.final_url),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_parsing() {
        assert_eq!(parse_content_range("bytes 0-99/1000"), Some(ContentRange { start: 0, end: 99, total: Some(1000) }));
        assert_eq!(parse_content_range("bytes 5-9/*"), Some(ContentRange { start: 5, end: 9, total: None }));
        assert_eq!(parse_content_range("bytes 10-5/100"), None);
        assert_eq!(parse_content_range("bytes 0-100/100"), None);
        assert_eq!(parse_content_range("bytes */100"), None);
        assert_eq!(parse_content_range("items 0-1/2"), None);
    }

    #[test]
    fn retry_after_parsing() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after("99999"), Some(Duration::from_secs(600)));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), Some(Duration::from_secs(0)));
        assert_eq!(parse_retry_after("soon"), None);
    }

    #[test]
    fn if_range_prefers_strong_etag() {
        assert_eq!(ResponseMeta::if_range_validator(Some("\"a\""), Some("date")).as_deref(), Some("\"a\""));
        assert_eq!(ResponseMeta::if_range_validator(Some("W/\"a\""), Some("date")).as_deref(), Some("date"));
        assert_eq!(ResponseMeta::if_range_validator(None, None), None);
    }
}
