//! Native messaging host for the Veltrix browser extension.
//!
//! The browser starts this process and talks to it over stdin/stdout using
//! length-prefixed JSON (native messaging). Every message is validated and
//! forwarded to the running desktop app over an authenticated local socket
//! or named pipe. The host itself never downloads anything and never opens
//! network connections.
//!
//! stdout is reserved for protocol frames; diagnostics go to stderr, which
//! browsers write to their own logs.

use odm_ipc::framing::{self, FrameError};
use odm_ipc::transport::{app_data_dir, Client, ClientError};
use odm_ipc::{Envelope, IpcErrorCode, Request, Response};
use std::io::{self, BufWriter};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const HOST_NAME: &str = "app.veltrix";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--manifest") {
        print_manifest(&args);
        return;
    }
    if args.get(1).map(String::as_str) == Some("--version") {
        println!("veltrix-native-host {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let origin = browser_origin(&args);
    let client = Client::new(app_data_dir());
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let mut launched = false;

    loop {
        let raw = match framing::read_frame(&mut input) {
            Ok(raw) => raw,
            Err(FrameError::Closed) => break,
            Err(e) => {
                eprintln!("veltrix-native-host: cannot read message: {e}");
                let _ =
                    framing::write_message(&mut output, &error("invalid", IpcErrorCode::BadRequest, &e.to_string()));
                break;
            }
        };
        let request: Envelope<Request> = match serde_json::from_slice(&raw) {
            Ok(r) => r,
            Err(e) => {
                let id = serde_json::from_slice::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(sanitize_id))
                    .unwrap_or_else(|| "invalid".into());
                let resp = error(&id, IpcErrorCode::BadRequest, &format!("invalid message: {e}"));
                if framing::write_message(&mut output, &resp).is_err() {
                    break;
                }
                continue;
            }
        };
        let response = match request.validate() {
            Err(e) => error(&request.id, IpcErrorCode::BadRequest, &e.0),
            Ok(()) => forward(&client, origin.clone(), request, &mut launched),
        };
        if framing::write_message(&mut output, &response).is_err() {
            break;
        }
    }
}

fn forward(
    client: &Client,
    origin: Option<String>,
    request: Envelope<Request>,
    launched: &mut bool,
) -> Envelope<Response> {
    let id = request.id.clone();
    // Only a concrete download offer may start the app; config lookups at
    // browser start must not launch it.
    let may_launch = matches!(request.body, Request::Offer(_));
    match client.request(origin.clone(), request.clone()) {
        Ok(resp) => return resp,
        Err(ClientError::NotRunning(_)) if may_launch && !*launched => {}
        Err(ClientError::NotRunning(_)) => return error(&id, IpcErrorCode::AppNotRunning, "Veltrix is not running"),
        Err(e) => return error(&id, IpcErrorCode::Internal, &e.to_string()),
    }
    *launched = true;
    if let Err(e) = launch_app() {
        eprintln!("veltrix-native-host: cannot start the app: {e}");
        return error(&id, IpcErrorCode::AppNotRunning, "Veltrix is not running and could not be started");
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        std::thread::sleep(Duration::from_millis(300));
        match client.request(origin.clone(), request.clone()) {
            Ok(resp) => return resp,
            Err(ClientError::NotRunning(_)) if Instant::now() < deadline => continue,
            Err(e) => return error(&id, IpcErrorCode::AppNotRunning, &e.to_string()),
        }
    }
}

fn error(id: &str, code: IpcErrorCode, message: &str) -> Envelope<Response> {
    Envelope::new(id, Response::error(code, message))
}

fn sanitize_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(64).collect()
}

/// Chrome passes the caller origin (`chrome-extension://<id>/`) as first
/// argument; Firefox passes the manifest path and the add-on id.
fn browser_origin(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|a| a.starts_with("chrome-extension://"))
        .or_else(|| args.get(2).filter(|a| a.contains('@') || a.starts_with('{')))
        .map(|s| s.chars().take(256).collect())
}

fn app_executable() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| io::Error::other("no parent directory"))?;
    let names: &[&str] = if cfg!(windows) { &["veltrix.exe", "Veltrix.exe"] } else { &["veltrix", "Veltrix"] };
    names
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "app executable not found next to the host"))
}

fn launch_app() -> io::Result<()> {
    let exe = app_executable()?;
    #[cfg(target_os = "macos")]
    {
        // Inside an app bundle: let LaunchServices start it properly.
        let s = exe.to_string_lossy();
        if let Some(pos) = s.find(".app/Contents/MacOS/") {
            let bundle = &s[..pos + 4];
            Command::new("open")
                .args(["-g", "-a", bundle, "--args", "--background"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            return Ok(());
        }
    }
    let mut cmd = Command::new(&exe);
    cmd.arg("--background").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // Browsers run hosts inside a job object; break away so the app
        // survives when the host exits. Fall back if breakaway is denied.
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        if cmd.spawn().is_ok() {
            return Ok(());
        }
        let mut cmd = Command::new(&exe);
        cmd.arg("--background").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        cmd.spawn()?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        cmd.spawn()?;
        Ok(())
    }
}

/// `veltrix-native-host --manifest chrome|firefox <extension-id>` prints a
/// native messaging manifest for manual registration.
fn print_manifest(args: &[String]) {
    let browser = args.get(2).map(String::as_str).unwrap_or("chrome");
    let ext_id = args.get(3).cloned().unwrap_or_default();
    let path = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let mut manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": "Veltrix native messaging host",
        "path": path,
        "type": "stdio",
    });
    if browser == "firefox" {
        manifest["allowed_extensions"] = serde_json::json!([ext_id]);
    } else {
        manifest["allowed_origins"] = serde_json::json!([format!("chrome-extension://{ext_id}/")]);
    }
    println!("{}", serde_json::to_string_pretty(&manifest).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_detection() {
        let chrome = vec!["host".into(), "chrome-extension://abc/".into()];
        assert_eq!(browser_origin(&chrome).as_deref(), Some("chrome-extension://abc/"));
        let chrome_win = vec!["host".into(), "chrome-extension://abc/".into(), "--parent-window=0".into()];
        assert_eq!(browser_origin(&chrome_win).as_deref(), Some("chrome-extension://abc/"));
        let firefox = vec!["host".into(), "/path/manifest.json".into(), "odm@ext".into()];
        assert_eq!(browser_origin(&firefox).as_deref(), Some("odm@ext"));
        assert_eq!(browser_origin(&["host".to_string()]), None);
    }

    #[test]
    fn ids_are_sanitized() {
        assert_eq!(sanitize_id("a-b_c\"; rm"), "a-b_crm");
    }
}
