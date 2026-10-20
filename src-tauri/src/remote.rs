//! Read-only status page for phones in the local network.
//!
//! Disabled by default. When enabled, a small HTTP server listens on the
//! configured port and serves one page plus `GET /api/state`. Every API call
//! needs the secret token from the QR code. The page only shows file names,
//! status and progress — never URLs, folders, cookies or other details — and
//! offers no way to change anything.

use crate::i18n::{tr, Lang, Text};
use odm_core::model::DownloadStatus;
use odm_core::{Manager, Settings};
use serde::Serialize;
use std::net::{SocketAddr, TcpListener as StdListener, UdpSocket};
use std::sync::{Arc, Mutex};
use tauri::async_runtime::JoinHandle;

const PAGE: &str = include_str!("remote_page.html");
const MAX_ITEMS: usize = 60;

#[derive(Default)]
pub struct RemoteServer {
    inner: Mutex<Running>,
}

#[derive(Default)]
struct Running {
    key: Option<(u16, String)>,
    task: Option<JoinHandle<()>>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteInfo {
    pub enabled: bool,
    pub running: bool,
    pub error: Option<String>,
    pub urls: Vec<String>,
    /// QR code (SVG markup) for the first URL.
    pub qr_svg: Option<String>,
}

impl RemoteServer {
    /// Start, restart or stop the server so that it matches the settings.
    pub fn apply(&self, manager: &Manager, settings: &Settings) {
        let mut r = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let wanted = (settings.remote_view_enabled && !settings.remote_view_token.is_empty())
            .then(|| (settings.remote_view_port, settings.remote_view_token.clone()));
        if wanted == r.key && r.task.is_some() == wanted.is_some() {
            return;
        }
        if let Some(task) = r.task.take() {
            task.abort();
            tracing::info!("remote status page stopped");
        }
        r.key = wanted.clone();
        r.error = None;
        let Some((port, token)) = wanted else { return };
        // Bind synchronously so a busy port is reported right away.
        let listener = match StdListener::bind(SocketAddr::from(([0, 0, 0, 0], port))) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(port, error = %e, "remote status page cannot listen");
                let text = tr(Lang::from_settings(settings), Text::PortUnavailable);
                r.error = Some(text.replace("{port}", &port.to_string()).replace("{error}", &e.to_string()));
                return;
            }
        };
        if let Err(e) = listener.set_nonblocking(true) {
            r.error = Some(e.to_string());
            return;
        }
        let app = router(manager.clone(), token);
        r.task = Some(tauri::async_runtime::spawn(async move {
            match tokio::net::TcpListener::from_std(listener) {
                Ok(l) => {
                    tracing::info!(port, "remote status page listening");
                    if let Err(e) = axum::serve(l, app).await {
                        tracing::warn!(error = %e, "remote status page stopped with an error");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "remote status page cannot start"),
            }
        }));
    }

    pub fn info(&self, settings: &Settings) -> RemoteInfo {
        let r = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let running = r.task.as_ref().map(|t| !t.inner().is_finished()).unwrap_or(false);
        let urls: Vec<String> = if settings.remote_view_token.is_empty() {
            vec![]
        } else {
            lan_addresses()
                .into_iter()
                .map(|ip| format!("http://{ip}:{}/#t={}", settings.remote_view_port, settings.remote_view_token))
                .collect()
        };
        let qr_svg = urls.first().and_then(|u| qr_svg(u));
        RemoteInfo { enabled: settings.remote_view_enabled, running, error: r.error.clone(), urls, qr_svg }
    }
}

pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// The address other devices in the LAN reach this computer at. Connecting a
/// UDP socket sends no packet; it only selects the outgoing interface.
fn lan_addresses() -> Vec<String> {
    let mut out = Vec::new();
    for target in ["192.168.0.1:9", "10.0.0.1:9", "8.8.8.8:9"] {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(target).is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    let ip = addr.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() && !out.contains(&ip.to_string()) {
                        out.push(ip.to_string());
                    }
                }
            }
        }
    }
    out
}

fn qr_svg(data: &str) -> Option<String> {
    let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
    Some(
        code.render::<qrcode::render::svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(qrcode::render::svg::Color("#111111"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build(),
    )
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

#[derive(Clone)]
struct Ctx {
    manager: Manager,
    token: Arc<str>,
}

pub(crate) fn router(manager: Manager, token: String) -> Router {
    Router::new().route("/", get(page)).route("/api/state", get(state)).with_state(Ctx { manager, token: token.into() })
}

async fn page() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'",
            ),
        ],
        PAGE,
    )
        .into_response()
}

#[derive(Serialize)]
struct Item {
    name: String,
    status: DownloadStatus,
    category: String,
    downloaded: u64,
    total: Option<u64>,
    speed: u64,
    eta: Option<u64>,
}

#[derive(Serialize)]
struct StateDto {
    total_speed: u64,
    active: usize,
    waiting: usize,
    completed: usize,
    failed: usize,
    downloads: Vec<Item>,
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    odm_ipc::transport::token_eq(given, token)
}

async fn state(State(ctx): State<Ctx>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &ctx.token) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let mut list = ctx.manager.list();
    let rank = |s: DownloadStatus| match s {
        DownloadStatus::Downloading | DownloadStatus::Connecting => 0,
        DownloadStatus::Waiting | DownloadStatus::Paused => 1,
        DownloadStatus::Failed => 2,
        _ => 3,
    };
    list.sort_by(|a, b| {
        rank(a.download.status).cmp(&rank(b.download.status)).then(b.download.updated_at.cmp(&a.download.updated_at))
    });
    let count = |f: &dyn Fn(DownloadStatus) -> bool| list.iter().filter(|d| f(d.download.status)).count();
    let dto = StateDto {
        total_speed: list.iter().filter(|d| d.download.status.is_active()).map(|d| d.speed).sum(),
        active: count(&|s| s.is_active()),
        waiting: count(&|s| matches!(s, DownloadStatus::Waiting | DownloadStatus::Paused)),
        completed: count(&|s| s == DownloadStatus::Completed),
        failed: count(&|s| s == DownloadStatus::Failed),
        downloads: list
            .into_iter()
            .take(MAX_ITEMS)
            .map(|v| Item {
                name: v.download.file_name,
                status: v.download.status,
                category: v.download.category,
                downloaded: v.download.downloaded,
                total: v.download.total_size,
                speed: v.speed,
                eta: v.eta_secs,
            })
            .collect(),
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(dto)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use odm_core::model::{AddDownloadRequest, StartMode};
    use odm_core::Store;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn get(addr: SocketAddr, path: &str, auth: Option<&str>) -> (u16, String) {
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let auth = auth.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n{auth}Connection: close\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).await.unwrap();
        let status = buf[9..12].parse().unwrap();
        let body = buf.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }

    #[tokio::test]
    async fn requires_token_and_hides_urls() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let mut s = store.load_settings().unwrap();
        s.download_dir = dir.path().to_path_buf();
        store.save_settings(&s).unwrap();
        let manager = Manager::start(store).unwrap();
        manager
            .add(AddDownloadRequest {
                url: "https://secret.example.com/file.iso?token=abc".into(),
                start: StartMode::Paused,
                ..Default::default()
            })
            .unwrap();
        let token = new_token();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(manager, token.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let (status, body) = get(addr, "/", None).await;
        assert_eq!(status, 200);
        assert!(body.contains("OpenDownload"));
        assert_eq!(get(addr, "/api/state", None).await.0, 401);
        assert_eq!(get(addr, "/api/state", Some("wrong")).await.0, 401);
        let (status, body) = get(addr, "/api/state", Some(&token)).await;
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("file.iso"));
        assert!(body.contains("\"waiting\":1"));
        assert!(!body.contains("secret.example.com"), "URLs must not be exposed: {body}");
        assert!(!body.contains(dir.path().to_string_lossy().as_ref()), "paths must not be exposed");
    }

    #[test]
    fn tokens_and_qr() {
        let t = new_token();
        assert_eq!(t.len(), 64);
        assert!(qr_svg("http://192.168.1.2:8765/#t=abc").unwrap().contains("<svg"));
    }
}
