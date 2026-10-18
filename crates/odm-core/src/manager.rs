//! Download manager: owns all downloads, runs the queue and scheduler,
//! persists state and publishes events for the UI.

use crate::category;
use crate::engine::{self, Outcome, Progress, StopSignal, TaskContext, PHASE_DOWNLOADING};
use crate::error::{DownloadError, ErrorCode, Result};
use crate::filename;
use crate::http::{self, HttpConfig};
use crate::limiter::RateLimiter;
use crate::model::*;
use crate::settings::{Settings, SpeedMode};
use crate::storage::Store;
use crate::url_util;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch, Notify, Semaphore};
use tokio::task::JoinHandle;

const REPORT_INTERVAL: Duration = Duration::from_millis(500);
const RESUME_IDS_KEY: &str = "resume_ids";
const BANDWIDTH_KEY: &str = "bandwidth_estimate";

#[derive(Clone)]
pub struct Manager {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    settings: RwLock<Settings>,
    entries: Mutex<HashMap<DownloadId, Entry>>,
    events: broadcast::Sender<EngineEvent>,
    client: RwLock<reqwest::Client>,
    limiter: Arc<RateLimiter>,
    connections: RwLock<Arc<Semaphore>>,
    queue_running: AtomicBool,
    peak_bps: AtomicU64,
    bandwidth_estimate: AtomicU64,
    wake: Notify,
    shutting_down: AtomicBool,
    schedule_state: Mutex<ScheduleState>,
}

#[derive(Default)]
struct ScheduleState {
    last_start_fired: Option<String>,
    last_stop_fired: Option<String>,
}

struct Entry {
    d: Download,
    auth: RequestAuth,
    run: Option<Running>,
    failed_at: Option<Instant>,
}

struct Running {
    stop: watch::Sender<StopSignal>,
    progress: Arc<Progress>,
    started: Instant,
    start_received: u64,
    last_received: u64,
    speed: f64,
    handle: Option<JoinHandle<()>>,
}

impl Entry {
    fn view(&self) -> DownloadView {
        let (speed, eta, conns) = match &self.run {
            Some(r) => {
                let speed = r.speed as u64;
                let received = r.progress.received.load(Ordering::Relaxed);
                let eta = match (r.progress.total(), speed) {
                    (Some(t), s) if s > 0 => Some(t.saturating_sub(received) / s),
                    _ => None,
                };
                (speed, eta, r.progress.active_connections.load(Ordering::Relaxed))
            }
            None => (0, None, 0),
        };
        let mut d = self.d.clone();
        if let Some(r) = &self.run {
            d.downloaded = r.progress.received.load(Ordering::Relaxed);
            d.total_size = r.progress.total().or(d.total_size);
        }
        DownloadView { domain: url_util::domain_of(d.final_url.as_deref().unwrap_or(&d.url)), download: d, speed, eta_secs: eta, connections_active: conns }
    }
}

impl Manager {
    /// Load state from the store and start the background loops.
    /// Must be called from within a Tokio runtime.
    pub fn start(store: Store) -> Result<Manager> {
        let settings = store.load_settings()?;
        let client = http::build_client(&http_config(&settings))?;
        let limits = settings.effective_limits();
        let (events, _) = broadcast::channel(1024);
        let estimate = store.get_value(BANDWIDTH_KEY)?.and_then(|v| v.parse().ok()).unwrap_or(0);
        let inner = Arc::new(Inner {
            limiter: Arc::new(RateLimiter::new(0)),
            connections: RwLock::new(Arc::new(Semaphore::new(limits.total_connections as usize))),
            queue_running: AtomicBool::new(settings.start_queue_on_startup),
            settings: RwLock::new(settings),
            store,
            entries: Mutex::new(HashMap::new()),
            events,
            client: RwLock::new(client),
            peak_bps: AtomicU64::new(0),
            bandwidth_estimate: AtomicU64::new(estimate),
            wake: Notify::new(),
            shutting_down: AtomicBool::new(false),
            schedule_state: Mutex::new(ScheduleState::default()),
        });
        let manager = Manager { inner };
        manager.apply_speed_mode();
        manager.recover()?;
        manager.spawn_loops();
        Ok(manager)
    }

    fn recover(&self) -> Result<()> {
        let settings = self.settings();
        let resume_ids: Vec<DownloadId> = self
            .inner
            .store
            .get_value(RESUME_IDS_KEY)?
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default();
        let mut to_start = Vec::new();
        {
            let mut entries = self.entries();
            for mut d in self.inner.store.list()? {
                let was_active = d.status.is_active() || resume_ids.contains(&d.id);
                if d.status.is_active() {
                    d.status = DownloadStatus::Paused;
                }
                if !matches!(d.status, DownloadStatus::Completed | DownloadStatus::Canceled)
                    && !d.segments.is_empty()
                    && !d.part_path().is_file()
                {
                    tracing::info!(id = %d.id, "partial file missing, download will start over");
                    d.segments.clear();
                    d.downloaded = 0;
                }
                if !d.resumable && !d.segments.is_empty() {
                    // Non-resumable data cannot be continued anyway.
                    d.segments.clear();
                    d.downloaded = 0;
                }
                if was_active && settings.resume_on_startup && d.status == DownloadStatus::Paused {
                    if d.in_queue {
                        d.status = DownloadStatus::Waiting;
                    } else {
                        to_start.push(d.id);
                    }
                }
                let _ = self.inner.store.upsert(&d);
                entries.insert(d.id, Entry { d, auth: RequestAuth::default(), run: None, failed_at: None });
            }
        }
        let _ = self.inner.store.set_value(RESUME_IDS_KEY, "[]");
        for id in to_start {
            if let Err(e) = self.spawn_task(id) {
                tracing::warn!(%id, error = %e, "could not resume download");
            }
        }
        Ok(())
    }

    fn spawn_loops(&self) {
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(REPORT_INTERVAL);
            loop {
                interval.tick().await;
                let Some(inner) = weak.upgrade() else { break };
                Manager { inner }.report_progress();
            }
        });
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                let Some(inner) = weak.upgrade() else { break };
                let manager = Manager { inner };
                manager.schedule_tick();
                let wake = manager.inner.clone();
                drop(manager);
                tokio::select! {
                    _ = wake.wake.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
            }
        });
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<DownloadId, Entry>> {
        self.inner.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.inner.events.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events.subscribe()
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    pub fn list(&self) -> Vec<DownloadView> {
        let mut v: Vec<DownloadView> = self.entries().values().map(Entry::view).collect();
        v.sort_by_key(|d| d.download.created_at);
        v
    }

    pub fn get(&self, id: DownloadId) -> Option<DownloadView> {
        self.entries().get(&id).map(Entry::view)
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn queue_running(&self) -> bool {
        self.inner.queue_running.load(Ordering::Relaxed)
    }

    /// Best known throughput in bytes/s for time estimates (0 = unknown).
    pub fn bandwidth_estimate(&self) -> u64 {
        let live: u64 = self.entries().values().filter_map(|e| e.run.as_ref()).map(|r| r.speed as u64).sum();
        live.max(self.inner.bandwidth_estimate.load(Ordering::Relaxed))
    }

    /// An unfinished download with the same URL, used for duplicate detection.
    pub fn find_unfinished_by_url(&self, url: &str) -> Option<DownloadId> {
        let normalized = url_util::validate_download_url(url).ok().map(|v| v.url.to_string());
        let normalized = normalized.as_deref().unwrap_or(url);
        self.entries()
            .values()
            .find(|e| {
                !matches!(e.d.status, DownloadStatus::Completed | DownloadStatus::Canceled)
                    && (e.d.url == normalized || e.d.final_url.as_deref() == Some(normalized))
            })
            .map(|e| e.d.id)
    }

    pub async fn probe(
        &self,
        url: &str,
        referrer: Option<&str>,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<ProbeInfo> {
        let v = url_util::validate_download_url(url)?;
        let auth = RequestAuth { basic: basic_from(v.credentials, username, password), cookie_header: None };
        let client = self.client();
        let custom = self.settings().custom_categories;
        http::probe(&client, v.url.as_str(), referrer, Some(&auth), &custom).await
    }

    fn client(&self) -> reqwest::Client {
        self.inner.client.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    // ------------------------------------------------------------------
    // Commands
    // ------------------------------------------------------------------

    pub fn add(&self, req: AddDownloadRequest) -> Result<DownloadView> {
        let settings = self.settings();
        let v = url_util::validate_download_url(&req.url)?;
        let url = v.url.to_string();
        if !req.allow_duplicate {
            if let Some(existing) = self.find_unfinished_by_url(&url) {
                return Err(DownloadError::new(
                    ErrorCode::Duplicate,
                    format!("This URL is already in the download list ({existing})"),
                )
                .retryable(false));
            }
        }
        let referrer = req
            .referrer
            .as_deref()
            .and_then(|r| url_util::validate_download_url(r).ok())
            .map(|r| r.url.to_string());
        let save_dir = filename::validate_directory(req.save_dir.as_deref().unwrap_or(&settings.download_dir))?;
        let expected_sha256 = match req.expected_sha256.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(h) if h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()) => Some(h.to_ascii_lowercase()),
            Some(_) => {
                return Err(DownloadError::new(ErrorCode::InvalidState, "The SHA-256 checksum must be 64 hex characters")
                    .retryable(false))
            }
            None => None,
        };
        let name_locked = req.file_name.as_deref().map(|n| !n.trim().is_empty()).unwrap_or(false);
        let mime = req.mime.clone().filter(|m| !m.is_empty() && m.len() < 256);
        let candidate = match req.file_name.as_deref().filter(|n| !n.trim().is_empty()) {
            Some(n) => filename::sanitize_file_name(n),
            None => filename::choose_file_name(None, Some(&v.url), None, mime.as_deref()),
        };
        let auth = RequestAuth {
            basic: basic_from(v.credentials, req.username.clone(), req.password.clone()),
            cookie_header: req.cookie_header.clone().filter(|c| !c.is_empty() && c.len() < 16 * 1024),
        };
        let now = now_ms();
        let view = {
            let mut entries = self.entries();
            let file_name = unique_name_in(&entries, &save_dir, &candidate, None);
            let queue_position = entries.values().map(|e| e.d.queue_position).max().unwrap_or(0) + 1;
            let d = Download {
                id: uuid::Uuid::new_v4(),
                url,
                final_url: None,
                referrer,
                category: category::classify(&file_name, mime.as_deref(), &settings.custom_categories),
                file_name,
                name_locked,
                save_dir,
                total_size: req.total_size,
                downloaded: 0,
                status: match req.start {
                    StartMode::Queue => DownloadStatus::Waiting,
                    _ => DownloadStatus::Paused,
                },
                error: None,
                priority: req.priority,
                in_queue: req.start == StartMode::Queue,
                queue_position,
                resumable: false,
                etag: None,
                last_modified: None,
                mime,
                segments: vec![],
                connections: req.connections.unwrap_or(0).min(32),
                expected_sha256,
                retry_count: 0,
                created_at: now,
                updated_at: now,
                completed_at: None,
                avg_speed: 0,
                active_ms: 0,
                requires_auth: !auth.is_empty(),
            };
            self.inner.store.upsert(&d)?;
            let entry = Entry { d, auth, run: None, failed_at: None };
            let view = entry.view();
            entries.insert(entry.d.id, entry);
            view
        };
        tracing::info!(id = %view.download.id, url = %url_util::redact_url(&view.download.url), "download added");
        self.emit(EngineEvent::Added { download: view.clone() });
        match req.start {
            StartMode::Now => self.spawn_task(view.download.id)?,
            StartMode::Queue => self.inner.wake.notify_one(),
            StartMode::Paused => {}
        }
        Ok(self.get(view.download.id).unwrap_or(view))
    }

    /// Start or continue a download immediately (outside the queue limit).
    pub fn resume(&self, id: DownloadId) -> Result<()> {
        {
            let mut entries = self.entries();
            let e = entries.get_mut(&id).ok_or_else(not_found)?;
            if e.run.is_some() {
                return Ok(());
            }
            if e.d.status == DownloadStatus::Completed {
                return Err(DownloadError::invalid_state("The download is already complete"));
            }
        }
        self.spawn_task(id)
    }

    pub fn pause(&self, id: DownloadId) -> Result<()> {
        let mut entries = self.entries();
        let e = entries.get_mut(&id).ok_or_else(not_found)?;
        if let Some(run) = &e.run {
            let _ = run.stop.send(StopSignal::Pause);
        } else if e.d.status == DownloadStatus::Waiting {
            e.d.status = DownloadStatus::Paused;
            e.d.updated_at = now_ms();
            self.inner.store.upsert(&e.d)?;
            let view = e.view();
            drop(entries);
            self.emit(EngineEvent::Updated { download: view });
        }
        Ok(())
    }

    /// Stop the download and delete the partial file. The entry stays in the list.
    pub fn cancel(&self, id: DownloadId) -> Result<()> {
        let mut entries = self.entries();
        let e = entries.get_mut(&id).ok_or_else(not_found)?;
        if let Some(run) = &e.run {
            let _ = run.stop.send(StopSignal::Cancel);
            return Ok(());
        }
        if e.d.status == DownloadStatus::Completed {
            return Err(DownloadError::invalid_state("A completed download cannot be canceled"));
        }
        let _ = std::fs::remove_file(e.d.part_path());
        e.d.status = DownloadStatus::Canceled;
        e.d.segments.clear();
        e.d.downloaded = 0;
        e.d.updated_at = now_ms();
        self.inner.store.upsert(&e.d)?;
        let view = e.view();
        drop(entries);
        self.emit(EngineEvent::Updated { download: view });
        Ok(())
    }

    /// Remove from the list. `delete_file` also deletes a completed file.
    /// Partial files of unfinished downloads are always deleted.
    pub async fn remove(&self, id: DownloadId, delete_file: bool) -> Result<()> {
        let handle = {
            let mut entries = self.entries();
            let e = entries.get_mut(&id).ok_or_else(not_found)?;
            match e.run.as_mut() {
                Some(run) => {
                    let _ = run.stop.send(StopSignal::Cancel);
                    run.handle.take()
                }
                None => None,
            }
        };
        if let Some(h) = handle {
            let _ = tokio::time::timeout(Duration::from_secs(10), h).await;
        }
        let removed = self.entries().remove(&id);
        if let Some(e) = removed {
            if e.d.status != DownloadStatus::Completed {
                let _ = std::fs::remove_file(e.d.part_path());
            } else if delete_file {
                let path = e.d.target_path();
                if path.is_file() {
                    std::fs::remove_file(&path).map_err(|err| DownloadError::io(&err, "Cannot delete file"))?;
                }
            }
            self.inner.store.delete(id)?;
            self.emit(EngineEvent::Removed { id });
        }
        Ok(())
    }

    /// Restart a failed or canceled download (keeps resumable progress).
    pub fn retry(&self, id: DownloadId) -> Result<()> {
        {
            let mut entries = self.entries();
            let e = entries.get_mut(&id).ok_or_else(not_found)?;
            if e.run.is_some() {
                return Ok(());
            }
            if e.d.status == DownloadStatus::Completed {
                // Download again from scratch into a new file.
                e.d.segments.clear();
                e.d.downloaded = 0;
                e.d.completed_at = None;
                e.d.file_name = unique_name_in(&HashMap::new(), &e.d.save_dir, &e.d.file_name, None);
            }
            e.d.error = None;
            e.d.retry_count = 0;
        }
        self.spawn_task(id)
    }

    /// Provide credentials for a download (e.g. after an app restart).
    pub fn set_credentials(&self, id: DownloadId, username: String, password: Option<String>) -> Result<()> {
        let mut entries = self.entries();
        let e = entries.get_mut(&id).ok_or_else(not_found)?;
        e.auth.basic = Some(url_util::BasicCredentials { username, password });
        Ok(())
    }

    pub fn set_priority(&self, id: DownloadId, priority: Priority) -> Result<()> {
        self.update_record(id, |d| d.priority = priority)?;
        self.inner.wake.notify_one();
        Ok(())
    }

    pub fn set_in_queue(&self, id: DownloadId, in_queue: bool) -> Result<()> {
        self.update_record(id, |d| {
            d.in_queue = in_queue;
            if in_queue && matches!(d.status, DownloadStatus::Paused | DownloadStatus::Failed | DownloadStatus::Canceled) {
                d.status = DownloadStatus::Waiting;
                d.error = None;
            } else if !in_queue && d.status == DownloadStatus::Waiting {
                d.status = DownloadStatus::Paused;
            }
        })?;
        self.inner.wake.notify_one();
        Ok(())
    }

    /// Move a download within the queue order. `delta` < 0 moves it up.
    pub fn move_in_queue(&self, id: DownloadId, delta: i64) -> Result<()> {
        let mut changed = Vec::new();
        {
            let mut entries = self.entries();
            let mut order: Vec<(i64, DownloadId)> =
                entries.values().filter(|e| e.d.in_queue).map(|e| (e.d.queue_position, e.d.id)).collect();
            order.sort();
            let Some(idx) = order.iter().position(|(_, i)| *i == id) else {
                return Err(DownloadError::invalid_state("The download is not in the queue"));
            };
            let new_idx = (idx as i64 + delta).clamp(0, order.len() as i64 - 1) as usize;
            let item = order.remove(idx);
            order.insert(new_idx, item);
            for (pos, (_, did)) in order.iter().enumerate() {
                if let Some(e) = entries.get_mut(did) {
                    if e.d.queue_position != pos as i64 {
                        e.d.queue_position = pos as i64;
                        e.d.updated_at = now_ms();
                        self.inner.store.upsert(&e.d)?;
                        changed.push(e.view());
                    }
                }
            }
        }
        for v in changed {
            self.emit(EngineEvent::Updated { download: v });
        }
        Ok(())
    }

    pub fn start_queue(&self) {
        self.inner.queue_running.store(true, Ordering::Relaxed);
        self.emit(EngineEvent::QueueState { running: true });
        self.inner.wake.notify_one();
    }

    /// Stop the queue: running queue downloads are paused and return to waiting.
    pub fn stop_queue(&self) {
        self.inner.queue_running.store(false, Ordering::Relaxed);
        let entries = self.entries();
        for e in entries.values() {
            if let (true, Some(run)) = (e.d.in_queue, &e.run) {
                let _ = run.stop.send(StopSignal::Pause);
            }
        }
        drop(entries);
        self.emit(EngineEvent::QueueState { running: false });
    }

    pub fn update_settings(&self, mut new: Settings) -> Result<Settings> {
        new.normalize();
        if new.download_dir.as_os_str().is_empty() {
            new.download_dir = crate::settings::default_download_dir();
        }
        if !new.download_dir.is_absolute() {
            return Err(DownloadError::new(ErrorCode::InvalidPath, "The download folder must be an absolute path")
                .retryable(false));
        }
        let old = self.settings();
        self.inner.store.save_settings(&new)?;
        if http_config_changed(&old, &new) {
            let client = http::build_client(&http_config(&new))?;
            *self.inner.client.write().unwrap_or_else(|p| p.into_inner()) = client;
        }
        if old.effective_limits().total_connections != new.effective_limits().total_connections {
            *self.inner.connections.write().unwrap_or_else(|p| p.into_inner()) =
                Arc::new(Semaphore::new(new.effective_limits().total_connections as usize));
        }
        if old.speed_mode != new.speed_mode {
            self.inner.peak_bps.store(0, Ordering::Relaxed);
        }
        *self.inner.settings.write().unwrap_or_else(|p| p.into_inner()) = new.clone();
        self.apply_speed_mode();
        self.inner.wake.notify_one();
        Ok(new)
    }

    /// Pause all running downloads and remember them for the next start.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::Relaxed);
        let (ids, handles): (Vec<DownloadId>, Vec<JoinHandle<()>>) = {
            let mut entries = self.entries();
            let mut ids = Vec::new();
            let mut handles = Vec::new();
            for e in entries.values_mut() {
                if let Some(run) = e.run.as_mut() {
                    let _ = run.stop.send(StopSignal::Pause);
                    ids.push(e.d.id);
                    if let Some(h) = run.handle.take() {
                        handles.push(h);
                    }
                }
            }
            (ids, handles)
        };
        let _ = self.inner.store.set_value(RESUME_IDS_KEY, &serde_json::to_string(&ids).unwrap_or_default());
        let all = futures_join(handles);
        let _ = tokio::time::timeout(Duration::from_secs(5), all).await;
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    fn update_record(&self, id: DownloadId, f: impl FnOnce(&mut Download)) -> Result<()> {
        let view = {
            let mut entries = self.entries();
            let e = entries.get_mut(&id).ok_or_else(not_found)?;
            f(&mut e.d);
            e.d.updated_at = now_ms();
            self.inner.store.upsert(&e.d)?;
            e.view()
        };
        self.emit(EngineEvent::Updated { download: view });
        Ok(())
    }

    fn apply_speed_mode(&self) {
        let s = self.settings();
        let rate = match s.speed_mode {
            SpeedMode::Maximum => 0,
            SpeedMode::Custom => s.speed_limit_bps,
            SpeedMode::Balanced => {
                let peak = self.inner.peak_bps.load(Ordering::Relaxed);
                if peak < 256 * 1024 {
                    0
                } else {
                    peak * 8 / 10
                }
            }
        };
        self.inner.limiter.set_rate(rate);
    }

    fn spawn_task(&self, id: DownloadId) -> Result<()> {
        let settings = self.settings();
        let limits = settings.effective_limits();
        let mut entries = self.entries();
        let e = entries.get_mut(&id).ok_or_else(not_found)?;
        if e.run.is_some() {
            return Ok(());
        }
        if matches!(e.d.status, DownloadStatus::Canceled | DownloadStatus::Completed) {
            e.d.segments.clear();
            e.d.downloaded = 0;
        }
        e.d.status = DownloadStatus::Connecting;
        e.d.error = None;
        e.d.updated_at = now_ms();
        self.inner.store.upsert(&e.d)?;

        let received: u64 = e.d.segments.iter().map(|s| s.written).sum();
        let progress = Arc::new(Progress::new(received, e.d.total_size));
        let (stop_tx, stop_rx) = watch::channel(StopSignal::Run);
        let weak = Arc::downgrade(&self.inner);
        let segments = if e.d.connections > 0 { e.d.connections.min(limits.segments.max(e.d.connections)) } else { limits.segments };
        let ctx = TaskContext {
            client: self.client(),
            limiter: self.inner.limiter.clone(),
            connections: self.inner.connections.read().unwrap_or_else(|p| p.into_inner()).clone(),
            max_segments: segments.clamp(1, 32),
            min_segment_size: settings.min_segment_size,
            max_retries: settings.max_retries,
            stop: stop_rx,
            progress: progress.clone(),
            auth: e.auth.clone(),
            custom_categories: settings.custom_categories.clone(),
            checkpoint: checkpoint_fn(weak.clone(), id),
            unique_name: unique_name_fn(weak.clone(), id),
        };
        let mut record = e.d.clone();
        let view = e.view();
        let task_weak = weak;
        let handle = tokio::spawn(async move {
            let result = engine::run(ctx, &mut record).await;
            if let Some(inner) = task_weak.upgrade() {
                Manager { inner }.on_task_finished(id, record, result);
            }
        });
        e.run = Some(Running {
            stop: stop_tx,
            progress,
            started: Instant::now(),
            start_received: received,
            last_received: received,
            speed: 0.0,
            handle: Some(handle),
        });
        drop(entries);
        tracing::info!(%id, "download started");
        self.emit(EngineEvent::Updated { download: view });
        Ok(())
    }

    fn on_task_finished(&self, id: DownloadId, record: Download, result: Result<Outcome>) {
        let settings = self.settings();
        let mut completed_event = None;
        let mut failed_event = None;
        let mut remove_completed = false;
        let view = {
            let mut entries = self.entries();
            let Some(e) = entries.get_mut(&id) else { return };
            let run = e.run.take();
            // Fields the user may have changed while the task was running.
            let (priority, in_queue, queue_position) = (e.d.priority, e.d.in_queue, e.d.queue_position);
            e.d = record;
            e.d.priority = priority;
            e.d.in_queue = in_queue;
            e.d.queue_position = queue_position;
            e.d.updated_at = now_ms();
            if let Some(run) = &run {
                let elapsed = run.started.elapsed();
                e.d.active_ms += elapsed.as_millis() as u64;
                let bytes = run.progress.received.load(Ordering::Relaxed).saturating_sub(run.start_received);
                if elapsed.as_millis() > 0 {
                    e.d.avg_speed = (bytes as f64 / elapsed.as_secs_f64()) as u64;
                }
            }
            match result {
                Ok(Outcome::Completed { path }) => {
                    e.d.status = DownloadStatus::Completed;
                    e.d.completed_at = Some(now_ms());
                    e.d.in_queue = false;
                    e.d.segments.clear();
                    e.d.error = None;
                    if e.d.avg_speed > 0 {
                        self.inner.bandwidth_estimate.store(e.d.avg_speed, Ordering::Relaxed);
                        let _ = self.inner.store.set_value(BANDWIDTH_KEY, &e.d.avg_speed.to_string());
                    }
                    tracing::info!(%id, "download completed");
                    completed_event = Some(EngineEvent::Completed { id, file_name: e.d.file_name.clone(), path });
                    remove_completed = settings.remove_completed_from_list;
                }
                Ok(Outcome::Paused) => {
                    let by_queue_stop = e.d.in_queue && !self.queue_running() && !self.inner.shutting_down.load(Ordering::Relaxed);
                    e.d.status = if by_queue_stop { DownloadStatus::Waiting } else { DownloadStatus::Paused };
                }
                Ok(Outcome::Canceled) => {
                    e.d.status = DownloadStatus::Canceled;
                    e.d.segments.clear();
                    e.d.downloaded = 0;
                }
                Err(err) => {
                    tracing::warn!(%id, code = ?err.code, error = %err.message, "download failed");
                    e.d.status = DownloadStatus::Failed;
                    let info = ErrorInfo::from(&err);
                    e.d.error = Some(info.clone());
                    e.failed_at = Some(Instant::now());
                    if !e.d.resumable {
                        e.d.segments.clear();
                        e.d.downloaded = 0;
                    }
                    failed_event = Some(EngineEvent::Failed { id, file_name: e.d.file_name.clone(), error: info });
                }
            }
            if let Err(err) = self.inner.store.upsert(&e.d) {
                tracing::error!(%id, error = %err, "cannot persist download state");
            }
            e.view()
        };
        self.emit(EngineEvent::Updated { download: view });
        if let Some(ev) = completed_event {
            self.emit(ev);
        }
        if let Some(ev) = failed_event {
            self.emit(ev);
        }
        if remove_completed {
            if self.entries().remove(&id).is_some() {
                let _ = self.inner.store.delete(id);
                self.emit(EngineEvent::Removed { id });
            }
        }
        self.inner.wake.notify_one();
    }

    fn report_progress(&self) {
        let mut items = Vec::new();
        let mut total_speed = 0f64;
        let mut status_changes = Vec::new();
        {
            let mut entries = self.entries();
            for e in entries.values_mut() {
                let Some(run) = e.run.as_mut() else { continue };
                let received = run.progress.received.load(Ordering::Relaxed);
                let delta = received.saturating_sub(run.last_received) as f64 / REPORT_INTERVAL.as_secs_f64();
                run.last_received = received;
                // Exponential moving average over roughly three seconds.
                run.speed = if run.speed == 0.0 { delta } else { run.speed * 0.8 + delta * 0.2 };
                if run.speed < 1.0 {
                    run.speed = 0.0;
                }
                total_speed += run.speed;
                if run.progress.phase.load(Ordering::Relaxed) == PHASE_DOWNLOADING
                    && e.d.status == DownloadStatus::Connecting
                {
                    e.d.status = DownloadStatus::Downloading;
                    status_changes.push(e.d.clone());
                }
                let total = run.progress.total();
                let speed = run.speed as u64;
                let elapsed = run.started.elapsed().as_secs_f64();
                items.push(ProgressItem {
                    id: e.d.id,
                    downloaded: received,
                    total_size: total,
                    speed,
                    avg_speed: if elapsed > 0.5 {
                        (received.saturating_sub(run.start_received) as f64 / elapsed) as u64
                    } else {
                        0
                    },
                    eta_secs: match total {
                        Some(t) if speed > 0 => Some(t.saturating_sub(received) / speed),
                        _ => None,
                    },
                    connections_active: run.progress.active_connections.load(Ordering::Relaxed),
                    status: e.d.status,
                });
            }
        }
        for d in status_changes {
            let _ = self.inner.store.upsert(&d);
        }
        let total = total_speed as u64;
        if total > self.inner.peak_bps.load(Ordering::Relaxed) {
            self.inner.peak_bps.store(total, Ordering::Relaxed);
            if self.settings().speed_mode == SpeedMode::Balanced {
                self.apply_speed_mode();
            }
        }
        if !items.is_empty() {
            self.emit(EngineEvent::Progress { items, total_speed: total });
        }
    }

    fn schedule_tick(&self) {
        let settings = self.settings();
        self.apply_schedule(&settings);

        // Automatic retry of downloads that failed with a temporary error.
        let mut retry_now = Vec::new();
        if settings.auto_retry_failed {
            let mut entries = self.entries();
            for e in entries.values_mut() {
                let due = e.failed_at.map(|t| t.elapsed() >= Duration::from_secs(settings.auto_retry_delay_secs)).unwrap_or(false);
                let retryable = e.d.error.as_ref().map(|er| er.retryable).unwrap_or(false);
                if e.d.status == DownloadStatus::Failed && due && retryable && e.d.retry_count < settings.auto_retry_max {
                    e.d.retry_count += 1;
                    e.failed_at = None;
                    if e.d.in_queue {
                        e.d.status = DownloadStatus::Waiting;
                        let _ = self.inner.store.upsert(&e.d);
                    } else {
                        retry_now.push(e.d.id);
                    }
                }
            }
        }
        for id in retry_now {
            let _ = self.spawn_task(id);
        }

        if !self.queue_running() {
            return;
        }
        let limits = settings.effective_limits();
        let to_start: Vec<DownloadId> = {
            let entries = self.entries();
            let running = entries.values().filter(|e| e.d.in_queue && e.run.is_some()).count() as u32;
            let free = limits.max_concurrent.saturating_sub(running) as usize;
            let mut waiting: Vec<&Entry> =
                entries.values().filter(|e| e.d.in_queue && e.run.is_none() && e.d.status == DownloadStatus::Waiting).collect();
            waiting.sort_by(|a, b| b.d.priority.cmp(&a.d.priority).then(a.d.queue_position.cmp(&b.d.queue_position)));
            waiting.iter().take(free).map(|e| e.d.id).collect()
        };
        for id in to_start {
            if let Err(e) = self.spawn_task(id) {
                tracing::warn!(%id, error = %e, "could not start queued download");
            }
        }
    }

    fn apply_schedule(&self, settings: &Settings) {
        let sched = &settings.schedule;
        if !sched.enabled {
            return;
        }
        use chrono::{Datelike, Local, Timelike};
        let now = Local::now();
        let weekday = now.weekday().num_days_from_monday() as u8;
        if !sched.days.is_empty() && !sched.days.contains(&weekday) {
            return;
        }
        let hm = format!("{:02}:{:02}", now.hour(), now.minute());
        let stamp = format!("{} {hm}", now.date_naive());
        let mut state = self.inner.schedule_state.lock().unwrap_or_else(|p| p.into_inner());
        if hm == sched.start_time && state.last_start_fired.as_deref() != Some(&stamp) {
            state.last_start_fired = Some(stamp.clone());
            drop(state);
            tracing::info!("scheduled queue start");
            self.start_queue();
            return;
        }
        if sched.stop_time.as_deref() == Some(hm.as_str()) && state.last_stop_fired.as_deref() != Some(&stamp) {
            state.last_stop_fired = Some(stamp);
            drop(state);
            tracing::info!("scheduled queue stop");
            self.stop_queue();
        }
    }

    /// Path of a completed download's file, if it still exists.
    pub fn completed_path(&self, id: DownloadId) -> Result<PathBuf> {
        let entries = self.entries();
        let e = entries.get(&id).ok_or_else(not_found)?;
        if e.d.status != DownloadStatus::Completed {
            return Err(DownloadError::invalid_state("The download is not complete yet"));
        }
        let p = e.d.target_path();
        if !p.exists() {
            return Err(DownloadError::new(ErrorCode::NotFoundLocal, "The file no longer exists").retryable(false));
        }
        Ok(p)
    }

    /// Folder of a download (for "show in folder" of unfinished items).
    pub fn location(&self, id: DownloadId) -> Result<PathBuf> {
        let entries = self.entries();
        let e = entries.get(&id).ok_or_else(not_found)?;
        let target = e.d.target_path();
        Ok(if target.exists() { target } else { e.d.save_dir.clone() })
    }
}

fn not_found() -> DownloadError {
    DownloadError::new(ErrorCode::NotFoundLocal, "Unknown download").retryable(false)
}

fn basic_from(
    from_url: Option<url_util::BasicCredentials>,
    username: Option<String>,
    password: Option<String>,
) -> Option<url_util::BasicCredentials> {
    match username.filter(|u| !u.is_empty()) {
        Some(username) => Some(url_util::BasicCredentials { username, password }),
        None => from_url,
    }
}

fn http_config(s: &Settings) -> HttpConfig {
    HttpConfig {
        connect_timeout: Duration::from_secs(s.connect_timeout_secs),
        read_timeout: Duration::from_secs(s.read_timeout_secs),
        allow_insecure_redirects: s.allow_insecure_redirects,
        user_agent: if s.user_agent.trim().is_empty() { http::DEFAULT_USER_AGENT.into() } else { s.user_agent.clone() },
    }
}

fn http_config_changed(a: &Settings, b: &Settings) -> bool {
    a.connect_timeout_secs != b.connect_timeout_secs
        || a.read_timeout_secs != b.read_timeout_secs
        || a.allow_insecure_redirects != b.allow_insecure_redirects
        || a.user_agent != b.user_agent
}

fn unique_name_in(
    entries: &HashMap<DownloadId, Entry>,
    dir: &Path,
    candidate: &str,
    except: Option<DownloadId>,
) -> String {
    filename::unique_file_name(dir, candidate, |name| {
        entries.values().any(|e| {
            Some(e.d.id) != except
                && e.d.status != DownloadStatus::Completed
                && e.d.save_dir == dir
                && e.d.file_name == name
        })
    })
}

fn checkpoint_fn(weak: Weak<Inner>, id: DownloadId) -> engine::CheckpointFn {
    Arc::new(move |snap: &Download| {
        let Some(inner) = weak.upgrade() else { return };
        let manager = Manager { inner };
        let mut entries = manager.entries();
        let Some(e) = entries.get_mut(&id) else { return };
        let renamed = e.d.file_name != snap.file_name || e.d.total_size != snap.total_size;
        e.d.segments = snap.segments.clone();
        e.d.downloaded = snap.downloaded;
        e.d.total_size = snap.total_size;
        e.d.final_url = snap.final_url.clone();
        e.d.etag = snap.etag.clone();
        e.d.last_modified = snap.last_modified.clone();
        e.d.mime = snap.mime.clone();
        e.d.file_name = snap.file_name.clone();
        e.d.category = snap.category.clone();
        e.d.resumable = snap.resumable;
        e.d.updated_at = now_ms();
        if let Err(err) = manager.inner.store.upsert(&e.d) {
            tracing::error!(%id, error = %err, "checkpoint failed");
        }
        if renamed {
            let view = e.view();
            drop(entries);
            manager.emit(EngineEvent::Updated { download: view });
        }
    })
}

fn unique_name_fn(weak: Weak<Inner>, id: DownloadId) -> engine::UniqueNameFn {
    Arc::new(move |dir: &Path, candidate: &str| {
        let Some(inner) = weak.upgrade() else { return filename::sanitize_file_name(candidate) };
        let manager = Manager { inner };
        let entries = manager.entries();
        unique_name_in(&entries, dir, candidate, Some(id))
    })
}

async fn futures_join(handles: Vec<JoinHandle<()>>) {
    for h in handles {
        let _ = h.await;
    }
}
