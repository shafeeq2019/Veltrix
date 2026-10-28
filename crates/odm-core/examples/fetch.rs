//! Diagnostic: download a URL with the real engine and print progress.
//! Usage: cargo run -p odm-core --example fetch -- <url> [seconds]

use odm_core::model::{AddDownloadRequest, StartMode};
use odm_core::{Manager, Store};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_env("ODM_LOG")).with_writer(std::io::stderr).init();
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("url");
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(40);
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let mut s = store.load_settings().unwrap();
    s.download_dir = dir.path().to_path_buf();
    store.save_settings(&s).unwrap();
    let manager = Manager::start(store).unwrap();
    let v = manager.add(AddDownloadRequest { url, start: StartMode::Now, ..Default::default() }).unwrap();
    let id = v.download.id;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Some(d) = manager.list().into_iter().find(|d| d.download.id == id) {
            println!(
                "{:>5.1}s status={:?} downloaded={} total={:?} speed={} conns={} segs={} err={:?}",
                start.elapsed().as_secs_f64(),
                d.download.status,
                d.download.downloaded,
                d.download.total_size,
                d.speed,
                d.connections_active,
                d.download.segments.len(),
                d.download.error.as_ref().map(|e| &e.message),
            );
            if !d.download.status.is_active() && d.download.status != odm_core::model::DownloadStatus::Waiting {
                break;
            }
        }
    }
    let _ = manager.pause(id);
    tokio::time::sleep(Duration::from_millis(500)).await;
}
