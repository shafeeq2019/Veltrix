//! Registration of the native messaging host with installed browsers.
//!
//! * Windows: manifest files in the app data folder plus `HKCU` registry
//!   keys pointing to them (no admin rights required).
//! * macOS / Linux: manifest files in each browser's per-user
//!   `NativeMessagingHosts` directory, only for browsers that are installed.

use odm_core::settings::FIREFOX_EXTENSION_ID;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const HOST_NAME: &str = "app.veltrix";
/// Host name registered by builds released as "OpenDownload Manager".
const LEGACY_HOST_NAME: &str = "org.opendownload.manager";

#[derive(Debug, Clone, Serialize)]
pub struct BrowserRegistration {
    pub browser: String,
    pub registered: bool,
    pub location: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationStatus {
    pub host_path: Option<String>,
    pub browsers: Vec<BrowserRegistration>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Chromium,
    Firefox,
}

struct Target {
    name: &'static str,
    kind: Kind,
    #[cfg(windows)]
    registry_key: &'static str,
    #[cfg(not(windows))]
    dir: PathBuf,
}

pub fn host_executable() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "veltrix-native-host.exe" } else { "veltrix-native-host" };
    let p = exe.parent()?.join(name);
    p.is_file().then_some(p)
}

fn manifest(kind: Kind, host: &Path, chromium_ids: &[String]) -> serde_json::Value {
    let mut m = serde_json::json!({
        "name": HOST_NAME,
        "description": "Veltrix native messaging host",
        "path": host.to_string_lossy(),
        "type": "stdio",
    });
    match kind {
        Kind::Chromium => {
            m["allowed_origins"] =
                serde_json::json!(chromium_ids.iter().map(|id| format!("chrome-extension://{id}/")).collect::<Vec<_>>())
        }
        Kind::Firefox => m["allowed_extensions"] = serde_json::json!([FIREFOX_EXTENSION_ID]),
    }
    m
}

#[cfg(not(windows))]
fn targets() -> Vec<Target> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut v = Vec::new();
    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library").join("Application Support");
        for (name, rel) in [
            ("Google Chrome", "Google/Chrome"),
            ("Chromium", "Chromium"),
            ("Microsoft Edge", "Microsoft Edge"),
            ("Brave", "BraveSoftware/Brave-Browser"),
            ("Vivaldi", "Vivaldi"),
        ] {
            v.push(Target { name, kind: Kind::Chromium, dir: base.join(rel).join("NativeMessagingHosts") });
        }
        v.push(Target { name: "Firefox", kind: Kind::Firefox, dir: base.join("Mozilla").join("NativeMessagingHosts") });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
        for (name, rel) in [
            ("Google Chrome", "google-chrome"),
            ("Chromium", "chromium"),
            ("Microsoft Edge", "microsoft-edge"),
            ("Brave", "BraveSoftware/Brave-Browser"),
            ("Vivaldi", "vivaldi"),
        ] {
            v.push(Target { name, kind: Kind::Chromium, dir: config.join(rel).join("NativeMessagingHosts") });
        }
        v.push(Target {
            name: "Firefox",
            kind: Kind::Firefox,
            dir: home.join(".mozilla").join("native-messaging-hosts"),
        });
    }
    v
}

#[cfg(windows)]
fn targets() -> Vec<Target> {
    vec![
        Target {
            name: "Google Chrome / Brave",
            kind: Kind::Chromium,
            registry_key: r"Software\Google\Chrome\NativeMessagingHosts",
        },
        Target { name: "Chromium", kind: Kind::Chromium, registry_key: r"Software\Chromium\NativeMessagingHosts" },
        Target {
            name: "Microsoft Edge",
            kind: Kind::Chromium,
            registry_key: r"Software\Microsoft\Edge\NativeMessagingHosts",
        },
        Target { name: "Firefox", kind: Kind::Firefox, registry_key: r"Software\Mozilla\NativeMessagingHosts" },
    ]
}

/// Write manifests and registry entries for all detected browsers.
pub fn register(data_dir: &Path, chromium_ids: &[String]) -> Result<IntegrationStatus, String> {
    let host = host_executable().ok_or("veltrix-native-host was not found next to the application")?;
    let manifests_dir = data_dir.join("native-messaging");
    std::fs::create_dir_all(&manifests_dir).map_err(|e| e.to_string())?;
    for kind in [Kind::Chromium, Kind::Firefox] {
        let path = manifest_file(&manifests_dir, kind);
        let json = serde_json::to_string_pretty(&manifest(kind, &host, chromium_ids)).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
    }
    for t in targets() {
        if let Err(e) = register_target(&t, &manifests_dir) {
            tracing::warn!(browser = t.name, error = %e, "native messaging registration failed");
        }
    }
    Ok(status(data_dir))
}

fn manifest_file(dir: &Path, kind: Kind) -> PathBuf {
    dir.join(match kind {
        Kind::Chromium => "chromium.json",
        Kind::Firefox => "firefox.json",
    })
}

#[cfg(not(windows))]
fn register_target(t: &Target, manifests_dir: &Path) -> std::io::Result<()> {
    // Only for installed browsers: their profile folder exists.
    let Some(browser_dir) = t.dir.parent() else { return Ok(()) };
    if !browser_dir.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&t.dir)?;
    std::fs::copy(manifest_file(manifests_dir, t.kind), t.dir.join(format!("{HOST_NAME}.json")))?;
    Ok(())
}

#[cfg(windows)]
fn register_target(t: &Target, manifests_dir: &Path) -> std::io::Result<()> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(format!(r"{}\{}", t.registry_key, HOST_NAME))?;
    key.set_value("", &manifest_file(manifests_dir, t.kind).to_string_lossy().to_string())?;
    Ok(())
}

/// Remove the registration of the former app name so browsers do not try to
/// start a host that no longer exists.
pub fn remove_legacy_registration() {
    for t in targets() {
        #[cfg(not(windows))]
        {
            let _ = std::fs::remove_file(t.dir.join(format!("{LEGACY_HOST_NAME}.json")));
        }
        #[cfg(windows)]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            let _ = winreg::RegKey::predef(HKEY_CURRENT_USER)
                .delete_subkey_all(format!(r"{}\{}", t.registry_key, LEGACY_HOST_NAME));
        }
    }
}

pub fn unregister(data_dir: &Path) -> IntegrationStatus {
    for t in targets() {
        #[cfg(not(windows))]
        {
            let _ = std::fs::remove_file(t.dir.join(format!("{HOST_NAME}.json")));
        }
        #[cfg(windows)]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            let _ = winreg::RegKey::predef(HKEY_CURRENT_USER)
                .delete_subkey_all(format!(r"{}\{}", t.registry_key, HOST_NAME));
        }
    }
    let _ = std::fs::remove_dir_all(data_dir.join("native-messaging"));
    status(data_dir)
}

pub fn status(data_dir: &Path) -> IntegrationStatus {
    let manifests_dir = data_dir.join("native-messaging");
    let browsers = targets()
        .into_iter()
        .map(|t| {
            #[cfg(not(windows))]
            {
                let file = t.dir.join(format!("{HOST_NAME}.json"));
                BrowserRegistration {
                    browser: t.name.into(),
                    registered: file.is_file(),
                    location: file.to_string_lossy().into(),
                }
            }
            #[cfg(windows)]
            {
                use winreg::enums::HKEY_CURRENT_USER;
                let key = format!(r"HKCU\{}\{}", t.registry_key, HOST_NAME);
                let registered = winreg::RegKey::predef(HKEY_CURRENT_USER)
                    .open_subkey(format!(r"{}\{}", t.registry_key, HOST_NAME))
                    .is_ok()
                    && manifest_file(&manifests_dir, t.kind).is_file();
                BrowserRegistration { browser: t.name.into(), registered, location: key }
            }
        })
        .collect();
    let _ = &manifests_dir;
    IntegrationStatus { host_path: host_executable().map(|p| p.to_string_lossy().into()), browsers }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_have_required_fields() {
        let host = Path::new("/opt/odm/veltrix-native-host");
        let c = manifest(Kind::Chromium, host, &["pjmfnhocabgijmcbdcbkfeaogjnhffea".into()]);
        assert_eq!(c["name"], HOST_NAME);
        assert_eq!(c["type"], "stdio");
        assert_eq!(c["allowed_origins"][0], "chrome-extension://pjmfnhocabgijmcbdcbkfeaogjnhffea/");
        let f = manifest(Kind::Firefox, host, &[]);
        assert_eq!(f["allowed_extensions"][0], FIREFOX_EXTENSION_ID);
        assert!(f.get("allowed_origins").is_none());
    }
}
