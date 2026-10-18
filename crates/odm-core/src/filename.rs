//! File name detection and sanitization.
//!
//! Every name that ends up on disk passes through [`sanitize_file_name`], so
//! names coming from servers, browsers or the user can never escape the
//! target directory or produce names Windows or macOS cannot handle.

use crate::error::{DownloadError, ErrorCode, Result};
use percent_encoding::percent_decode_str;
use std::path::{Path, PathBuf};

/// Suffix for files that are still being downloaded.
pub const PART_SUFFIX: &str = ".odmpart";
/// Keep names well below the 255 byte limit of NTFS/APFS so that the part
/// suffix and a " (123)" de-duplication counter always fit.
const MAX_NAME_BYTES: usize = 200;
pub const FALLBACK_NAME: &str = "download";

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Extract the file name from a `Content-Disposition` header.
/// Supports RFC 6266 / RFC 5987 `filename*` (preferred) and `filename`.
pub fn parse_content_disposition(header: &str) -> Option<String> {
    let mut plain: Option<String> = None;
    let mut extended: Option<String> = None;
    for part in split_params(header) {
        let Some((key, value)) = part.split_once('=') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "filename*" => {
                // charset'lang'percent-encoded
                let mut it = value.trim_matches('"').splitn(3, '\'');
                let charset = it.next().unwrap_or("").to_ascii_lowercase();
                let _lang = it.next();
                if let Some(encoded) = it.next() {
                    let bytes: Vec<u8> = percent_decode_str(encoded).collect();
                    let decoded = if charset == "utf-8" || charset.is_empty() {
                        String::from_utf8_lossy(&bytes).into_owned()
                    } else {
                        // ISO-8859-1: every byte is the code point.
                        bytes.iter().map(|&b| b as char).collect()
                    };
                    if !decoded.is_empty() {
                        extended = Some(decoded);
                    }
                }
            }
            "filename" => {
                let v = if value.starts_with('"') { unquote(value) } else { value.to_string() };
                // Many servers send percent-encoded UTF-8 in the plain parameter.
                let v = if v.contains('%') {
                    percent_decode_str(&v).decode_utf8().map(|c| c.into_owned()).unwrap_or(v)
                } else {
                    v
                };
                if !v.is_empty() {
                    plain = Some(v);
                }
            }
            _ => {}
        }
    }
    extended.or(plain)
}

/// Split a header value on `;` while respecting quoted strings.
fn split_params(header: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for c in header.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_quotes => {
                current.push(c);
                escaped = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                current.push(c);
            }
            ';' if !in_quotes => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
}

fn unquote(value: &str) -> String {
    let inner = value.strip_prefix('"').unwrap_or(value);
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Last non-empty path segment of a URL, percent-decoded.
pub fn name_from_url(url: &url::Url) -> Option<String> {
    let segment = url.path_segments()?.rev().find(|s| !s.is_empty())?;
    let decoded = percent_decode_str(segment).decode_utf8_lossy().into_owned();
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

/// Best-effort extension for a MIME type, used when neither the header nor
/// the URL provides a name with an extension.
pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    let essence = mime.split(';').next()?.trim().to_ascii_lowercase();
    Some(match essence.as_str() {
        "application/zip" | "application/x-zip-compressed" => "zip",
        "application/x-7z-compressed" => "7z",
        "application/vnd.rar" | "application/x-rar-compressed" => "rar",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-tar" => "tar",
        "application/pdf" => "pdf",
        "application/x-msdownload" | "application/vnd.microsoft.portable-executable" => "exe",
        "application/x-msi" | "application/x-ms-installer" => "msi",
        "application/x-apple-diskimage" => "dmg",
        "application/x-iso9660-image" => "iso",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/x-matroska" => "mkv",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "text/plain" => "txt",
        "application/json" => "json",
        _ => return None,
    })
}

/// Lower-cased extension (without dot) of a file name.
pub fn extension_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.len() > 10 || ext.contains(' ') {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Turn an arbitrary string into a single safe file name component that is
/// valid on Windows and macOS (and Linux).
pub fn sanitize_file_name(input: &str) -> String {
    // Only keep the last component: defeats "../../etc/passwd" and "C:\x\y".
    let base = input.rsplit(['/', '\\']).next().unwrap_or("");
    let mut name: String = base
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            // Unicode bidi overrides can disguise extensions ("exe.txt").
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => '_',
            c => c,
        })
        .collect();
    // Windows silently strips trailing dots and spaces; leading spaces are confusing.
    name = name.trim_start_matches(' ').trim_end_matches(['.', ' ']).to_string();
    if name.is_empty() || name == "." || name == ".." || name.chars().all(|c| c == '.' || c == '_') {
        name = FALLBACK_NAME.to_string();
    }
    let stem_upper = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    if WINDOWS_RESERVED.contains(&stem_upper.as_str()) {
        name = format!("_{name}");
    }
    truncate_preserving_extension(&name, MAX_NAME_BYTES)
}

fn truncate_preserving_extension(name: &str, max_bytes: usize) -> String {
    if name.len() <= max_bytes {
        return name.to_string();
    }
    let (stem, ext) = match extension_of(name) {
        Some(ext) => (&name[..name.len() - ext.len() - 1], format!(".{}", &name[name.len() - ext.len()..])),
        None => (name, String::new()),
    };
    let budget = max_bytes.saturating_sub(ext.len());
    let mut cut = budget.min(stem.len());
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    let stem = stem[..cut].trim_end_matches(['.', ' ']);
    format!("{stem}{ext}")
}

/// Determine a file name from the available sources in order of reliability.
pub fn choose_file_name(
    content_disposition: Option<&str>,
    final_url: Option<&url::Url>,
    original_url: Option<&url::Url>,
    mime: Option<&str>,
) -> String {
    let candidate = content_disposition
        .and_then(parse_content_disposition)
        .or_else(|| final_url.and_then(name_from_url))
        .or_else(|| original_url.and_then(name_from_url));
    let mut name = sanitize_file_name(candidate.as_deref().unwrap_or(FALLBACK_NAME));
    if extension_of(&name).is_none() {
        if let Some(ext) = mime.and_then(extension_for_mime) {
            name = sanitize_file_name(&format!("{name}.{ext}"));
        }
    }
    name
}

/// Join a sanitized name onto a directory and verify the result stays inside it.
pub fn safe_join(dir: &Path, name: &str) -> Result<PathBuf> {
    let clean = sanitize_file_name(name);
    let path = dir.join(&clean);
    if path.parent() != Some(dir) || path.file_name().map(|n| n.to_string_lossy() != clean).unwrap_or(true) {
        return Err(DownloadError::new(ErrorCode::InvalidPath, "Invalid file name").retryable(false));
    }
    Ok(path)
}

/// Validate a user supplied target directory.
pub fn validate_directory(dir: &Path) -> Result<PathBuf> {
    if !dir.is_absolute() {
        return Err(
            DownloadError::new(ErrorCode::InvalidPath, "The download folder must be an absolute path").retryable(false)
        );
    }
    if dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(
            DownloadError::new(ErrorCode::InvalidPath, "The download folder must not contain '..'").retryable(false)
        );
    }
    std::fs::create_dir_all(dir).map_err(|e| DownloadError::io(&e, "Cannot create download folder"))?;
    if !dir.is_dir() {
        return Err(
            DownloadError::new(ErrorCode::InvalidPath, "The download folder is not a directory").retryable(false)
        );
    }
    Ok(dir.to_path_buf())
}

/// Path of the temporary file for a final path.
pub fn part_path(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_os_string();
    s.push(PART_SUFFIX);
    PathBuf::from(s)
}

/// Return `name` or `name (n).ext` such that neither the file nor its part
/// file exists and `is_taken` (names reserved by other downloads) is false.
pub fn unique_file_name(dir: &Path, name: &str, is_taken: impl Fn(&str) -> bool) -> String {
    let name = sanitize_file_name(name);
    let free = |n: &str| {
        let p = dir.join(n);
        !p.exists() && !part_path(&p).exists() && !is_taken(n)
    };
    if free(&name) {
        return name;
    }
    let (stem, ext) = match extension_of(&name) {
        Some(ext) => (name[..name.len() - ext.len() - 1].to_string(), format!(".{}", &name[name.len() - ext.len()..])),
        None => (name.clone(), String::new()),
    };
    for i in 1..10_000 {
        let candidate = format!("{stem} ({i}){ext}");
        if free(&candidate) {
            return candidate;
        }
    }
    format!("{stem} ({}){ext}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_disposition_variants() {
        assert_eq!(parse_content_disposition("attachment; filename=\"a b.zip\"").as_deref(), Some("a b.zip"));
        assert_eq!(parse_content_disposition("attachment; filename=plain.iso").as_deref(), Some("plain.iso"));
        assert_eq!(
            parse_content_disposition("attachment; filename=\"fallback.txt\"; filename*=UTF-8''%E2%82%AC%20rates.txt")
                .as_deref(),
            Some("€ rates.txt")
        );
        assert_eq!(parse_content_disposition("attachment; filename*=iso-8859-1'en'%E4.txt").as_deref(), Some("ä.txt"));
        assert_eq!(parse_content_disposition("attachment; filename=\"x\\\"y;z.zip\"").as_deref(), Some("x\"y;z.zip"));
        assert_eq!(parse_content_disposition("inline").as_deref(), None);
        assert_eq!(parse_content_disposition("attachment; filename=\"%C3%BC.pdf\"").as_deref(), Some("ü.pdf"));
    }

    #[test]
    fn sanitize_blocks_traversal_and_reserved() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name("..\\..\\Windows\\system32\\evil.dll"), "evil.dll");
        assert_eq!(sanitize_file_name(".."), FALLBACK_NAME);
        assert_eq!(sanitize_file_name(""), FALLBACK_NAME);
        assert_eq!(sanitize_file_name("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_file_name("nul"), "_nul");
        assert_eq!(sanitize_file_name("a<b>c:d\"e|f?g*h.zip"), "a_b_c_d_e_f_g_h.zip");
        assert_eq!(sanitize_file_name("name. . ."), "name");
        assert_eq!(sanitize_file_name("bad\u{0}\u{1f}name.txt"), "bad__name.txt");
        assert_eq!(sanitize_file_name("invoice\u{202E}fdp.exe"), "invoice_fdp.exe");
        assert_eq!(sanitize_file_name("Über Größe.pdf"), "Über Größe.pdf");
    }

    #[test]
    fn long_names_keep_extension() {
        let long = format!("{}.tar.gz", "ä".repeat(300));
        let s = sanitize_file_name(&long);
        assert!(s.len() <= MAX_NAME_BYTES);
        assert!(s.ends_with(".gz"));
    }

    #[test]
    fn chooses_name_from_sources() {
        let u = url::Url::parse("https://x.org/dl/My%20File.zip?token=1").unwrap();
        assert_eq!(choose_file_name(None, Some(&u), None, None), "My File.zip");
        assert_eq!(choose_file_name(Some("attachment; filename=real.7z"), Some(&u), None, None), "real.7z");
        let bare = url::Url::parse("https://x.org/").unwrap();
        assert_eq!(choose_file_name(None, Some(&bare), None, Some("application/pdf")), "download.pdf");
        let noext = url::Url::parse("https://x.org/get").unwrap();
        assert_eq!(choose_file_name(None, Some(&noext), None, Some("application/zip")), "get.zip");
    }

    #[test]
    fn safe_join_stays_inside() {
        let dir = std::env::temp_dir();
        let p = safe_join(&dir, "../../x.txt").unwrap();
        assert_eq!(p.parent().unwrap(), dir.as_path());
        assert_eq!(p.file_name().unwrap(), "x.txt");
    }

    #[test]
    fn unique_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.zip"), b"x").unwrap();
        std::fs::write(dir.path().join("a (1).zip.odmpart"), b"x").unwrap();
        assert_eq!(unique_file_name(dir.path(), "a.zip", |_| false), "a (2).zip");
        assert_eq!(unique_file_name(dir.path(), "b.zip", |n| n == "b.zip"), "b (1).zip");
        assert_eq!(unique_file_name(dir.path(), "noext", |_| false), "noext");
    }

    #[test]
    fn rejects_relative_directory() {
        assert!(validate_directory(Path::new("relative/dir")).is_err());
    }
}
