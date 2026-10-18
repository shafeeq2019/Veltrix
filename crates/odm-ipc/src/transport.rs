//! Local IPC endpoint, authentication token and blocking client.
//!
//! * Unix (macOS/Linux): a Unix domain socket inside the per-user app data
//!   directory (mode 0700), socket mode 0600, and the peer's UID is checked.
//! * Windows: a named pipe `\\.\pipe\opendownload-manager-<user>` created
//!   with `first_pipe_instance` (prevents squatting) and
//!   `reject_remote_clients`.
//!
//! In addition every request must carry a random 256-bit token that the app
//! writes to a file only the current user can read. Nothing listens on a
//! TCP port.

use crate::framing::{self, FrameError};
use crate::protocol::{AppRequest, Envelope, Request, Response};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const APP_IDENTIFIER: &str = "org.opendownload.manager";
const TOKEN_FILE: &str = "ipc-token";

/// Per-user application data directory, identical to Tauri's `app_data_dir`.
pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ODM_DATA_DIR") {
        return PathBuf::from(dir);
    }
    base_data_dir().join(APP_IDENTIFIER)
}

fn base_data_dir() -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home().join("AppData").join("Roaming"))
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home().join(".local").join("share"))
    }
}

pub fn token_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TOKEN_FILE)
}

/// Name of the endpoint for the given data directory.
pub fn endpoint(data_dir: &Path) -> String {
    #[cfg(windows)]
    {
        let _ = data_dir;
        let user: String = std::env::var("USERNAME")
            .unwrap_or_else(|_| "user".into())
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        format!(r"\\.\pipe\opendownload-manager-{user}")
    }
    #[cfg(not(windows))]
    {
        let p = data_dir.join("ipc.sock");
        // sun_path is limited to 104 (macOS) / 108 (Linux) bytes.
        if p.as_os_str().len() < 100 {
            p.to_string_lossy().into_owned()
        } else {
            let uid = unsafe { libc::getuid() };
            std::env::temp_dir().join(format!("odm-{uid}.sock")).to_string_lossy().into_owned()
        }
    }
}

/// Create a fresh random token and store it with user-only permissions.
pub fn write_new_token(data_dir: &Path) -> io::Result<String> {
    std::fs::create_dir_all(data_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(data_dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let path = token_path(data_dir);
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        let mut f = opts.open(&tmp)?;
        f.write_all(token.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(token)
}

pub fn read_token(data_dir: &Path) -> io::Result<String> {
    let t = std::fs::read_to_string(token_path(data_dir))?;
    let t = t.trim().to_string();
    if t.len() != 64 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid token file"));
    }
    Ok(t)
}

/// Constant-time comparison so the token cannot be guessed byte by byte.
pub fn token_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("the desktop app is not running")]
    NotRunning(#[source] io::Error),
    #[error("authentication token unavailable: {0}")]
    Token(#[source] io::Error),
    #[error("IPC failure: {0}")]
    Frame(#[from] FrameError),
}

/// Blocking client used by the native messaging host.
pub struct Client {
    data_dir: PathBuf,
    pub timeout: Duration,
}

impl Client {
    pub fn new(data_dir: PathBuf) -> Self {
        Client { data_dir, timeout: Duration::from_secs(150) }
    }

    pub fn request(&self, origin: Option<String>, request: Envelope<Request>) -> Result<Envelope<Response>, ClientError> {
        let token = read_token(&self.data_dir).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                ClientError::NotRunning(e)
            } else {
                ClientError::Token(e)
            }
        })?;
        let msg = AppRequest { token, origin, request };
        let mut stream = self.connect().map_err(ClientError::NotRunning)?;
        framing::write_message(&mut stream, &msg)?;
        Ok(framing::read_message(&mut stream)?)
    }

    #[cfg(unix)]
    fn connect(&self) -> io::Result<std::os::unix::net::UnixStream> {
        let s = std::os::unix::net::UnixStream::connect(endpoint(&self.data_dir))?;
        s.set_read_timeout(Some(self.timeout))?;
        s.set_write_timeout(Some(Duration::from_secs(10)))?;
        Ok(s)
    }

    #[cfg(windows)]
    fn connect(&self) -> io::Result<std::fs::File> {
        let name = endpoint(&self.data_dir);
        // ERROR_PIPE_BUSY (231): all instances busy, retry briefly.
        for _ in 0..50 {
            match std::fs::OpenOptions::new().read(true).write(true).open(&name) {
                Ok(f) => return Ok(f),
                Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "named pipe busy"))
    }
}
