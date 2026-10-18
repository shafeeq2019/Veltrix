//! URL validation and redaction.

use crate::error::{DownloadError, ErrorCode, Result};
use url::Url;

pub const MAX_URL_LEN: usize = 16 * 1024;

/// Credentials that were embedded in a URL (`https://user:pass@host/`).
/// They are moved out of the URL so they are never persisted or logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicCredentials {
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidatedUrl {
    pub url: Url,
    pub credentials: Option<BasicCredentials>,
}

/// Parse and validate a user or browser supplied download URL.
///
/// Only `http` and `https` are accepted. Embedded credentials are stripped
/// from the URL and returned separately. Fragments are removed because they
/// are never sent to the server.
pub fn validate_download_url(input: &str) -> Result<ValidatedUrl> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(DownloadError::new(ErrorCode::InvalidUrl, "The URL is empty").retryable(false));
    }
    if trimmed.len() > MAX_URL_LEN {
        return Err(DownloadError::new(ErrorCode::InvalidUrl, "The URL is too long").retryable(false));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(
            DownloadError::new(ErrorCode::InvalidUrl, "The URL contains control characters").retryable(false)
        );
    }
    let mut url = Url::parse(trimmed)
        .map_err(|e| DownloadError::new(ErrorCode::InvalidUrl, format!("Invalid URL: {e}")).retryable(false))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(DownloadError::new(
                ErrorCode::UnsupportedScheme,
                format!("Unsupported URL scheme '{other}', only http and https are supported"),
            )
            .retryable(false))
        }
    }
    if url.host_str().map(str::is_empty).unwrap_or(true) {
        return Err(DownloadError::new(ErrorCode::InvalidUrl, "The URL has no host").retryable(false));
    }
    let credentials = if !url.username().is_empty() || url.password().is_some() {
        let username = percent_encoding::percent_decode_str(url.username()).decode_utf8_lossy().into_owned();
        let password = url
            .password()
            .map(|p| percent_encoding::percent_decode_str(p).decode_utf8_lossy().into_owned());
        let _ = url.set_username("");
        let _ = url.set_password(None);
        Some(BasicCredentials { username, password })
    } else {
        None
    };
    url.set_fragment(None);
    Ok(ValidatedUrl { url, credentials })
}

/// A representation of the URL that is safe to write into logs: no
/// credentials, no query string (signed URLs carry tokens there).
pub fn redact_url(url: &str) -> String {
    match Url::parse(url) {
        Ok(u) => {
            let mut out = format!("{}://{}", u.scheme(), u.host_str().unwrap_or("?"));
            if let Some(port) = u.port() {
                out.push_str(&format!(":{port}"));
            }
            out.push_str(u.path());
            if u.query().is_some() {
                out.push_str("?<redacted>");
            }
            out
        }
        Err(_) => "<invalid url>".to_string(),
    }
}

/// Host part of the URL, used for the UI and for exclusion rules.
pub fn domain_of(url: &str) -> Option<String> {
    Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
}

/// Matches `host` against an exclusion pattern. `example.com` matches the
/// host itself and all sub-domains; `*.example.com` only sub-domains.
pub fn domain_matches(host: &str, pattern: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let pattern = pattern.trim().trim_end_matches('.').to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host.ends_with(&format!(".{suffix}"));
    }
    host == pattern || host.ends_with(&format!(".{pattern}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https() {
        assert!(validate_download_url("https://example.com/a.zip").is_ok());
        assert!(validate_download_url("  http://example.com/a.zip?x=1  ").is_ok());
    }

    #[test]
    fn rejects_other_schemes() {
        for u in ["file:///etc/passwd", "ftp://x/y", "javascript:alert(1)", "data:text/plain,hi", "chrome://settings"] {
            let err = validate_download_url(u).unwrap_err();
            assert!(matches!(err.code, ErrorCode::UnsupportedScheme | ErrorCode::InvalidUrl), "{u}");
        }
    }

    #[test]
    fn rejects_garbage_and_control_chars() {
        assert!(validate_download_url("").is_err());
        assert!(validate_download_url("not a url").is_err());
        assert!(validate_download_url("https://exa\nmple.com/").is_err());
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        assert!(validate_download_url(&long).is_err());
    }

    #[test]
    fn strips_credentials_and_fragment() {
        let v = validate_download_url("https://us%40er:p%3Ass@example.com/f.iso#frag").unwrap();
        assert_eq!(v.url.as_str(), "https://example.com/f.iso");
        let c = v.credentials.unwrap();
        assert_eq!(c.username, "us@er");
        assert_eq!(c.password.as_deref(), Some("p:ss"));
    }

    #[test]
    fn redacts_query() {
        assert_eq!(
            redact_url("https://user:pw@cdn.example.com:8443/f.zip?token=secret"),
            "https://cdn.example.com:8443/f.zip?<redacted>"
        );
        assert_eq!(redact_url("garbage"), "<invalid url>");
    }

    #[test]
    fn domain_matching() {
        assert!(domain_matches("example.com", "example.com"));
        assert!(domain_matches("dl.example.com", "example.com"));
        assert!(!domain_matches("badexample.com", "example.com"));
        assert!(domain_matches("a.example.com", "*.example.com"));
        assert!(!domain_matches("example.com", "*.example.com"));
        assert!(!domain_matches("example.com", ""));
    }
}
