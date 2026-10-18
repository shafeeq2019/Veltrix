//! End-to-end tests of the manager and engine against a local HTTP server.

mod common;

use common::{expected, TestServer};
use odm_core::model::{AddDownloadRequest, DownloadStatus, StartMode};
use odm_core::{ErrorCode, Manager, Settings, Store};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

struct Env {
    _dir: tempfile::TempDir,
    downloads: PathBuf,
    db: PathBuf,
}

fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let db = dir.path().join("odm.sqlite");
    Env { downloads, db, _dir: dir }
}

fn manager(env: &Env, tweak: impl FnOnce(&mut Settings)) -> Manager {
    let store = Store::open(&env.db).unwrap();
    let mut s = store.load_settings().unwrap();
    s.download_dir = env.downloads.clone();
    s.min_segment_size = 64 * 1024;
    s.segments_per_download = 4;
    s.max_retries = 4;
    s.auto_retry_failed = false;
    s.read_timeout_secs = 10;
    tweak(&mut s);
    store.save_settings(&s).unwrap();
    Manager::start(store).unwrap()
}

fn add(m: &Manager, url: String) -> uuid::Uuid {
    m.add(AddDownloadRequest { url, ..Default::default() }).unwrap().download.id
}

async fn wait_for(m: &Manager, id: uuid::Uuid, want: &[DownloadStatus], secs: u64) -> odm_core::DownloadView {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let v = m.get(id).expect("download exists");
        if want.contains(&v.download.status) {
            return v;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("timeout waiting for {want:?}, current: {:?} {:?}", v.download.status, v.download.error);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn assert_file(path: &Path, size: u64, variant: u64) {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"));
    assert_eq!(data.len() as u64, size, "size of {path:?}");
    assert!(data == expected(size, variant), "content of {path:?} differs");
    assert!(!odm_core::filename::part_path(path).exists(), "part file left behind");
}

const MB: u64 = 1024 * 1024;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn segmented_download_is_byte_exact() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = 5 * MB + 12345;
    let id = add(&m, server.url(&format!("/file/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), size, 0);
    assert!(server.stats.range_requests.load(Ordering::SeqCst) >= 4, "expected parallel range requests");
    assert!(server.stats.max_concurrent.load(Ordering::SeqCst) >= 2, "expected concurrent connections");
    assert_eq!(v.download.file_name, size.to_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn falls_back_to_single_connection_without_ranges() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = 3 * MB;
    let id = add(&m, server.url(&format!("/norange/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), size, 0);
    assert!(!v.download.resumable);
    assert_eq!(server.stats.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unknown_size_download() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = MB + 7;
    let id = add(&m, server.url(&format!("/unknown/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), size, 0);
    assert_eq!(v.download.total_size, Some(size));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resumes_after_connection_aborts() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |s| s.segments_per_download = 1);
    let size = 2 * MB;
    let id = add(&m, server.url(&format!("/flaky/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), size, 0);
    assert!(server.stats.requests.load(Ordering::SeqCst) >= 3);
    // the retries continued from the received offset instead of restarting
    assert!(server.stats.range_requests.load(Ordering::SeqCst) >= 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_capped_ranges_are_continued() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |s| s.segments_per_download = 2);
    let size = MB;
    let id = add(&m, server.url(&format!("/capped/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), size, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_resume_and_cancel() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = 8 * MB;
    let id = add(&m, server.url(&format!("/slow/{size}")));
    // wait until some data arrived and a checkpoint was written
    loop {
        let v = m.get(id).unwrap();
        if v.download.downloaded > MB && !v.download.segments.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(2200)).await;
    m.pause(id).unwrap();
    let v = wait_for(&m, id, &[DownloadStatus::Paused], 10).await;
    assert!(v.download.downloaded > 0);
    assert!(v.download.resumable);
    assert!(v.download.part_path().exists());
    let before = server.stats.range_requests.load(Ordering::SeqCst);

    m.resume(id).unwrap();
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 60).await;
    assert_file(&v.download.target_path(), size, 0);
    assert!(server.stats.range_requests.load(Ordering::SeqCst) > before);

    // cancel removes the partial file
    let id2 = m
        .add(AddDownloadRequest { url: server.url(&format!("/slow/{size}")), allow_duplicate: true, ..Default::default() })
        .unwrap()
        .download
        .id;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let part = m.get(id2).unwrap().download.part_path();
    m.cancel(id2).unwrap();
    wait_for(&m, id2, &[DownloadStatus::Canceled], 10).await;
    assert!(!part.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn state_survives_restart() {
    let server = TestServer::start().await;
    let env = env();
    let size = 8 * MB;
    let id;
    {
        let m = manager(&env, |_| {});
        id = add(&m, server.url(&format!("/slow/{size}")));
        tokio::time::sleep(Duration::from_millis(2500)).await;
        m.shutdown().await;
        let v = m.get(id).unwrap();
        assert_eq!(v.download.status, DownloadStatus::Paused);
    }
    let m = manager(&env, |_| {});
    let v = m.get(id).unwrap();
    assert_eq!(v.download.status, DownloadStatus::Paused);
    assert!(v.download.downloaded > 0, "progress restored");
    assert!(!v.download.segments.is_empty());
    m.resume(id).unwrap();
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 60).await;
    assert_file(&v.download.target_path(), size, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn changed_resource_restarts_cleanly() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = 2 * MB;
    let id = add(&m, server.url(&format!("/changing/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    // The final file must consist entirely of the new version.
    assert_file(&v.download.target_path(), size, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn follows_redirects() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = MB;
    let id = add(&m, server.url(&format!("/redirect/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), size, 0);
    assert!(v.download.final_url.unwrap().contains("/file/"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_errors_are_typed() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |s| s.max_retries = 1);
    let id = add(&m, server.url("/status/404"));
    let v = wait_for(&m, id, &[DownloadStatus::Failed], 30).await;
    let err = v.download.error.unwrap();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(!err.retryable);
    assert_eq!(server.stats.requests.load(Ordering::SeqCst), 1, "404 must not be retried");

    let id = add(&m, server.url("/status/503"));
    let v = wait_for(&m, id, &[DownloadStatus::Failed], 30).await;
    assert_eq!(v.download.error.unwrap().code, ErrorCode::ServerError);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rate_limited_then_succeeds() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = MB;
    let id = add(&m, server.url(&format!("/limited/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), size, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn content_disposition_name_and_duplicates() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let size = 100_000;
    let url = server.url(&format!("/disposition/{size}"));
    let id = add(&m, url.clone());
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_eq!(v.download.file_name, "report final.pdf");
    assert_eq!(v.download.category, "documents");
    // completed: the same URL may be added again and gets a unique name
    let id2 = add(&m, url.clone());
    let v2 = wait_for(&m, id2, &[DownloadStatus::Completed], 30).await;
    assert_eq!(v2.download.file_name, "report final (1).pdf");
    assert_file(&v2.download.target_path(), size, 0);
    // unfinished duplicates are rejected
    let slow = server.url("/slow/50000000");
    add(&m, slow.clone());
    let err = m.add(AddDownloadRequest { url: slow, ..Default::default() }).unwrap_err();
    assert_eq!(err.code, ErrorCode::Duplicate);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn checksum_mismatch_is_detected() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let req = AddDownloadRequest {
        url: server.url("/file/1000"),
        expected_sha256: Some("0".repeat(64)),
        ..Default::default()
    };
    let id = m.add(req).unwrap().download.id;
    let v = wait_for(&m, id, &[DownloadStatus::Failed], 30).await;
    assert_eq!(v.download.error.unwrap().code, ErrorCode::ChecksumMismatch);

    let data = expected(1000, 0);
    let path = env.downloads.join("ok.bin");
    std::fs::write(&path, &data).unwrap();
    let good = odm_core::engine::sha256_file(&path).unwrap();
    let req = AddDownloadRequest {
        url: server.url("/file/1000"),
        expected_sha256: Some(good),
        allow_duplicate: true,
        ..Default::default()
    };
    let id = m.add(req).unwrap().download.id;
    wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_file() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |_| {});
    let id = add(&m, server.url("/file/0"));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    assert_file(&v.download.target_path(), 0, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queue_respects_concurrency_limit() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |s| {
        s.max_concurrent_downloads = 1;
        s.segments_per_download = 1;
    });
    let ids: Vec<_> = (0..3)
        .map(|i| {
            m.add(AddDownloadRequest {
                url: server.url(&format!("/slow/{}", 300_000 + i)),
                start: StartMode::Queue,
                ..Default::default()
            })
            .unwrap()
            .download
            .id
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(ids.iter().all(|id| m.get(*id).unwrap().download.status == DownloadStatus::Waiting));
    m.start_queue();
    let mut max_active = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let list = m.list();
        let active = list.iter().filter(|v| v.download.status.is_active()).count();
        max_active = max_active.max(active);
        if list.iter().all(|v| v.download.status == DownloadStatus::Completed) {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "queue did not finish");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(max_active, 1);
    for (i, id) in ids.iter().enumerate() {
        let v = m.get(*id).unwrap();
        assert_file(&v.download.target_path(), 300_000 + i as u64, 0);
        assert!(!v.download.in_queue, "completed downloads leave the queue");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn custom_speed_limit_is_enforced() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |s| {
        s.speed_mode = odm_core::settings::SpeedMode::Custom;
        s.speed_limit_bps = 512 * 1024;
    });
    let size = 1536 * 1024;
    let start = std::time::Instant::now();
    let id = add(&m, server.url(&format!("/file/{size}")));
    let v = wait_for(&m, id, &[DownloadStatus::Completed], 30).await;
    let elapsed = start.elapsed();
    assert_file(&v.download.target_path(), size, 0);
    // 1.5 MiB at 512 KiB/s ≈ 3 s (minus the burst allowance)
    assert!(elapsed >= Duration::from_millis(2300), "finished too fast: {elapsed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rejects_invalid_input() {
    let env = env();
    let m = manager(&env, |_| {});
    for url in ["ftp://example.com/x", "file:///etc/passwd", "not a url", ""] {
        assert!(m.add(AddDownloadRequest { url: url.into(), ..Default::default() }).is_err(), "{url}");
    }
    let err = m
        .add(AddDownloadRequest {
            url: "http://127.0.0.1:9/x".into(),
            save_dir: Some(PathBuf::from("relative/path")),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidPath);
    // traversal in a user supplied name stays inside the folder
    let v = m
        .add(AddDownloadRequest {
            url: "http://127.0.0.1:9/x".into(),
            file_name: Some("../../evil.sh".into()),
            start: StartMode::Paused,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(v.download.file_name, "evil.sh");
    assert_eq!(v.download.save_dir, env.downloads);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_parallel_downloads() {
    let server = TestServer::start().await;
    let env = env();
    let m = manager(&env, |s| s.max_total_connections = 8);
    let sizes: Vec<u64> = (0..8).map(|i| 700_000 + i * 31_337).collect();
    let ids: Vec<_> = sizes.iter().map(|s| add(&m, server.url(&format!("/file/{s}")))).collect();
    for (id, size) in ids.iter().zip(&sizes) {
        let v = wait_for(&m, *id, &[DownloadStatus::Completed], 60).await;
        assert_file(&v.download.target_path(), *size, 0);
    }
    // the global connection cap holds across downloads
    assert!(server.stats.max_concurrent.load(Ordering::SeqCst) <= 8);
}
