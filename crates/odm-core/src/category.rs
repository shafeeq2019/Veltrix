//! Classification of downloads into categories.

use crate::filename::extension_of;
use serde::{Deserialize, Serialize};

pub const VIDEO: &str = "video";
pub const MUSIC: &str = "music";
pub const DOCUMENTS: &str = "documents";
pub const PROGRAMS: &str = "programs";
pub const ARCHIVES: &str = "archives";
pub const IMAGES: &str = "images";
pub const OTHER: &str = "other";

const BUILTIN: &[(&str, &[&str])] = &[
    (VIDEO, &["mp4", "mkv", "avi", "mov", "wmv", "webm", "flv", "m4v", "mpg", "mpeg", "3gp", "ts"]),
    (MUSIC, &["mp3", "flac", "wav", "aac", "ogg", "oga", "m4a", "wma", "opus", "aiff"]),
    (DOCUMENTS, &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "txt", "rtf", "epub", "csv", "md"]),
    (PROGRAMS, &["exe", "msi", "msix", "appx", "dmg", "pkg", "app", "deb", "rpm", "appimage", "apk", "bat", "cmd", "sh", "jar"]),
    (ARCHIVES, &["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "iso", "img", "cab"]),
    (IMAGES, &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "svg", "heic"]),
];

/// User defined category, matched before the built-in ones.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomCategory {
    pub id: String,
    pub name: String,
    pub extensions: Vec<String>,
}

pub fn classify(file_name: &str, mime: Option<&str>, custom: &[CustomCategory]) -> String {
    if let Some(ext) = extension_of(file_name) {
        for c in custom {
            if c.extensions.iter().any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext)) {
                return c.id.clone();
            }
        }
        for (cat, exts) in BUILTIN {
            if exts.contains(&ext.as_str()) {
                return (*cat).to_string();
            }
        }
    }
    if let Some(mime) = mime {
        let m = mime.to_ascii_lowercase();
        if m.starts_with("video/") {
            return VIDEO.into();
        }
        if m.starts_with("audio/") {
            return MUSIC.into();
        }
        if m.starts_with("image/") {
            return IMAGES.into();
        }
    }
    OTHER.into()
}

/// File types that can run code when opened. Opening them requires an
/// explicit confirmation in the UI.
pub fn is_executable_type(file_name: &str) -> bool {
    const EXEC: &[&str] = &[
        "exe", "msi", "msix", "appx", "bat", "cmd", "com", "scr", "ps1", "vbs", "js", "jse", "wsf", "hta", "lnk",
        "reg", "dmg", "pkg", "app", "command", "sh", "jar", "py", "pif", "cpl", "appimage", "deb", "rpm",
    ];
    extension_of(file_name).map(|e| EXEC.contains(&e.as_str())).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies() {
        assert_eq!(classify("a.MP4", None, &[]), VIDEO);
        assert_eq!(classify("setup.exe", None, &[]), PROGRAMS);
        assert_eq!(classify("x.7z", None, &[]), ARCHIVES);
        assert_eq!(classify("noext", Some("audio/mpeg"), &[]), MUSIC);
        assert_eq!(classify("noext", None, &[]), OTHER);
        let custom = vec![CustomCategory { id: "c1".into(), name: "Torrents".into(), extensions: vec![".torrent".into(), "zip".into()] }];
        assert_eq!(classify("a.torrent", None, &custom), "c1");
        assert_eq!(classify("a.zip", None, &custom), "c1");
    }

    #[test]
    fn executables() {
        assert!(is_executable_type("Setup.EXE"));
        assert!(is_executable_type("x.dmg"));
        assert!(!is_executable_type("x.pdf"));
        assert!(!is_executable_type("exe"));
    }
}
