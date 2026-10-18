//! The download task: probing, segment planning, parallel range requests,
//! work stealing, retries, checkpoints and finalization.
//!
//! Data flow per connection: response chunk → bounded in-memory buffer
//! (≤ [`FLUSH_SIZE`]) → positional write into the `.odmpart` file → the
//! segment's `written` counter. Only `written` is persisted, and only after
//! the data was synced to disk, so a crash can lose progress but never
//! produce a file with holes that the database claims are filled.

use crate::category;
use crate::error::{DownloadError, ErrorCode, Result};
use crate::filename;
use crate::http::{self, RequestSpec, ResponseMeta};
use crate::limiter::RateLimiter;
use crate::model::{Download, RequestAuth, SegmentState};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinSet;

const FLUSH_SIZE: usize = 512 * 1024;
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopSignal {
    Run,
    Pause,
    Cancel,
}

pub const PHASE_CONNECTING: u8 = 0;
pub const PHASE_DOWNLOADING: u8 = 1;

/// Live counters shared between a running task and the progress reporter.
#[derive(Debug, Default)]
pub struct Progress {
    pub received: AtomicU64,
    pub total: AtomicU64, // u64::MAX = unknown
    pub active_connections: AtomicU32,
    pub phase: AtomicU8,
}

impl Progress {
    pub fn new(received: u64, total: Option<u64>) -> Self {
        let p = Progress::default();
        p.received.store(received, Ordering::Relaxed);
        p.total.store(total.unwrap_or(u64::MAX), Ordering::Relaxed);
        p
    }

    pub fn total(&self) -> Option<u64> {
        match self.total.load(Ordering::Relaxed) {
            u64::MAX => None,
            t => Some(t),
        }
    }
}

pub type CheckpointFn = Arc<dyn Fn(&Download) + Send + Sync>;
pub type UniqueNameFn = Arc<dyn Fn(&Path, &str) -> String + Send + Sync>;

/// Everything a task needs from the manager.
#[derive(Clone)]
pub struct TaskContext {
    pub client: Client,
    pub limiter: Arc<RateLimiter>,
    pub connections: Arc<Semaphore>,
    pub max_segments: u32,
    pub min_segment_size: u64,
    pub max_retries: u32,
    pub stop: watch::Receiver<StopSignal>,
    pub progress: Arc<Progress>,
    pub auth: RequestAuth,
    pub custom_categories: Vec<category::CustomCategory>,
    /// Persist intermediate state (segments, metadata).
    pub checkpoint: CheckpointFn,
    /// Return a file name in `dir` that is not used by anything else.
    pub unique_name: UniqueNameFn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Completed { path: PathBuf },
    Paused,
    Canceled,
}

/// Run a download until it completes, fails or is stopped. `d` is updated
/// in place and reflects the final state.
pub async fn run(ctx: TaskContext, d: &mut Download) -> Result<Outcome> {
    filename::validate_directory(&d.save_dir)?;
    let mut restarted = false;
    loop {
        match run_once(&ctx, d).await {
            Err(e) if e.code == ErrorCode::ResourceChanged && !restarted => {
                tracing::warn!(id = %d.id, "resource changed on the server, restarting from scratch");
                restarted = true;
                reset(d);
                ctx.progress.received.store(0, Ordering::Relaxed);
            }
            other => return other,
        }
    }
}

fn reset(d: &mut Download) {
    d.segments.clear();
    d.downloaded = 0;
    d.etag = None;
    d.last_modified = None;
    d.resumable = false;
}

async fn run_once(ctx: &TaskContext, d: &mut Download) -> Result<Outcome> {
    let mut stop = ctx.stop.clone();
    ctx.progress.phase.store(PHASE_CONNECTING, Ordering::Relaxed);

    let can_resume = !d.segments.is_empty() && d.resumable && d.part_path().is_file();
    if !can_resume && !d.segments.is_empty() {
        tracing::info!(id = %d.id, "previous data cannot be resumed, starting over");
        reset(d);
    }
    ctx.progress.received.store(d.segments.iter().map(|s| s.written).sum(), Ordering::Relaxed);

    let mut initial = None;
    let file;
    if d.segments.is_empty() {
        let permit = acquire_permit(ctx, &mut stop).await?;
        let Some(permit) = permit else { return Ok(stopped_outcome(&stop, d)) };
        let url = d.url.clone();
        let spec = RequestSpec {
            url: &url,
            referrer: d.referrer.as_deref(),
            auth: Some(&ctx.auth),
            range: Some((0, None)),
            if_range: None,
        };
        let opened = send_with_retry(ctx, &spec, &mut stop).await?;
        let Some((resp, meta)) = opened else { return Ok(stopped_outcome(&stop, d)) };
        apply_metadata(ctx, d, &meta);
        file = Arc::new(create_part_file(d)?);
        let total = d.total_size;
        if total == Some(0) {
            drop(resp);
            return finalize(ctx, d, &file).await;
        }
        d.segments = plan_segments(total, d.resumable, ctx.max_segments.max(1), ctx.min_segment_size);
        initial = Some(InitialResponse { resp, meta, permit });
        (ctx.checkpoint)(d);
    } else {
        file = Arc::new(
            OpenOptions::new()
                .write(true)
                .open(d.part_path())
                .map_err(|e| DownloadError::io(&e, "Cannot open partial file"))?,
        );
        if let Some(total) = d.total_size {
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            if len != total {
                file.set_len(total).map_err(|e| DownloadError::io(&e, "Cannot resize partial file"))?;
            }
        }
    }

    let result = run_segments(ctx, d, file.clone(), initial).await;
    d.downloaded = d.segments.iter().map(|s| s.written).sum();
    match result {
        Ok(SegmentsResult::Done) => finalize(ctx, d, &file).await,
        Ok(SegmentsResult::Stopped) => {
            sync_file(&file).await;
            Ok(stopped_outcome(&ctx.stop, d))
        }
        Err(e) => {
            sync_file(&file).await;
            Err(e)
        }
    }
}

struct InitialResponse {
    resp: reqwest::Response,
    meta: ResponseMeta,
    permit: tokio::sync::OwnedSemaphorePermit,
}

fn stopped_outcome(stop: &watch::Receiver<StopSignal>, d: &Download) -> Outcome {
    match *stop.borrow() {
        StopSignal::Cancel => {
            let _ = std::fs::remove_file(d.part_path());
            Outcome::Canceled
        }
        _ => Outcome::Paused,
    }
}

async fn wait_stop(stop: &mut watch::Receiver<StopSignal>) {
    loop {
        if *stop.borrow_and_update() != StopSignal::Run {
            return;
        }
        if stop.changed().await.is_err() {
            // Sender dropped: the manager is gone, behave like pause.
            return;
        }
    }
}

async fn acquire_permit(
    ctx: &TaskContext,
    stop: &mut watch::Receiver<StopSignal>,
) -> Result<Option<tokio::sync::OwnedSemaphorePermit>> {
    tokio::select! {
        p = ctx.connections.clone().acquire_owned() => {
            p.map(Some).map_err(|_| DownloadError::internal("connection pool closed"))
        }
        _ = wait_stop(stop) => Ok(None),
    }
}

/// Send a request, retrying temporary failures with exponential backoff.
/// Returns `None` if stopped while waiting.
async fn send_with_retry(
    ctx: &TaskContext,
    spec: &RequestSpec<'_>,
    stop: &mut watch::Receiver<StopSignal>,
) -> Result<Option<(reqwest::Response, ResponseMeta)>> {
    let mut attempt = 0u32;
    loop {
        let result = match http::send(&ctx.client, spec).await {
            // Empty resources answer "bytes=0-" with 416.
            Err(e) if e.code == ErrorCode::RangeNotSatisfiable && spec.range == Some((0, None)) => {
                http::send(&ctx.client, &RequestSpec { range: None, ..spec.clone() }).await
            }
            r => r,
        };
        match result {
            Ok(r) => return Ok(Some(r)),
            Err(e) if e.retryable && attempt < ctx.max_retries => {
                attempt += 1;
                let delay = e.retry_after.unwrap_or_else(|| backoff(attempt));
                tracing::debug!(attempt, ?delay, error = %e.message, "request failed, retrying");
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = wait_stop(stop) => return Ok(None),
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// Exponential backoff with ±20 % jitter: 1 s, 2 s, 4 s … capped at 60 s.
pub fn backoff(attempt: u32) -> Duration {
    let base = 1000u64.saturating_mul(1u64 << attempt.saturating_sub(1).min(6)).min(60_000);
    let jitter = (base as f64 * (rand::random::<f64>() * 0.4 - 0.2)) as i64;
    Duration::from_millis((base as i64 + jitter).max(100) as u64)
}

fn apply_metadata(ctx: &TaskContext, d: &mut Download, meta: &ResponseMeta) {
    d.final_url = (meta.final_url != d.url).then(|| meta.final_url.clone());
    d.etag = meta.etag.clone();
    d.last_modified = meta.last_modified.clone();
    let mime = meta.mime.as_ref().map(|m| m.split(';').next().unwrap_or("").trim().to_string());
    if mime.is_some() {
        d.mime = mime;
    }
    let total = meta.total_size();
    if total.is_some() || d.total_size.is_none() {
        d.total_size = total;
    }
    ctx.progress.total.store(d.total_size.unwrap_or(u64::MAX), Ordering::Relaxed);
    // Resumable means: the server honoured the range and told us the full size.
    d.resumable = meta.status == 206 && meta.content_range.map(|r| r.start == 0 && r.total.is_some()).unwrap_or(false);
    if !d.name_locked {
        let original = url::Url::parse(&d.url).ok();
        let final_url = url::Url::parse(&meta.final_url).ok();
        let candidate = filename::choose_file_name(
            meta.content_disposition.as_deref(),
            final_url.as_ref(),
            original.as_ref(),
            d.mime.as_deref(),
        );
        if candidate != d.file_name {
            d.file_name = (ctx.unique_name)(&d.save_dir, &candidate);
        }
        d.category = category::classify(&d.file_name, d.mime.as_deref(), &ctx.custom_categories);
    }
}

fn create_part_file(d: &Download) -> Result<File> {
    let path = d.part_path();
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .map_err(|e| DownloadError::io(&e, "Cannot create file"))?;
    if let Some(total) = d.total_size {
        // Reserve the full size up front; positional writes fill it in.
        file.set_len(total).map_err(|e| DownloadError::io(&e, "Cannot allocate file"))?;
    }
    Ok(file)
}

/// Split `total` bytes into at most `max` segments of at least `min_size`.
pub fn plan_segments(total: Option<u64>, resumable: bool, max: u32, min_size: u64) -> Vec<SegmentState> {
    let Some(total) = total else { return vec![SegmentState { start: 0, end: None, written: 0 }] };
    if total == 0 {
        return vec![];
    }
    let count = if resumable { (total / min_size.max(1)).clamp(1, max as u64) } else { 1 };
    let size = total / count;
    (0..count)
        .map(|i| {
            let start = i * size;
            let end = if i == count - 1 { total - 1 } else { start + size - 1 };
            SegmentState { start, end: Some(end), written: 0 }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Segment workers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct SegRt {
    state: SegmentState,
    /// Bytes accepted from the network (written + buffered).
    received: u64,
    active: bool,
    /// Finished stream of unknown length (possibly zero bytes).
    done: bool,
}

struct Shared {
    table: Mutex<Vec<SegRt>>,
    file: Arc<File>,
    ctx: TaskContext,
    url: String,
    referrer: Option<String>,
    resumable: bool,
    total: Option<u64>,
    etag: Option<String>,
    if_range: Option<String>,
    abort: AtomicBool,
}

enum SegmentsResult {
    Done,
    Stopped,
}

enum SegExit {
    Done,
    Stopped,
}

async fn run_segments(
    ctx: &TaskContext,
    d: &mut Download,
    file: Arc<File>,
    initial: Option<InitialResponse>,
) -> Result<SegmentsResult> {
    let table: Vec<SegRt> = d
        .segments
        .iter()
        .map(|s| SegRt { state: *s, received: s.written, active: false, done: false })
        .collect();
    let shared = Arc::new(Shared {
        table: Mutex::new(table),
        file,
        ctx: ctx.clone(),
        // Segments go to the resolved URL so all connections hit the same
        // mirror; the original URL is used again when the task restarts.
        url: d.final_url.clone().unwrap_or_else(|| d.url.clone()),
        referrer: d.referrer.clone(),
        resumable: d.resumable,
        total: d.total_size,
        etag: d.etag.clone(),
        if_range: ResponseMeta::if_range_validator(d.etag.as_deref(), d.last_modified.as_deref()),
        abort: AtomicBool::new(false),
    });

    let can_split = d.resumable && d.total_size.is_some();
    let workers = if can_split { ctx.max_segments.max(1) as usize } else { 1 };

    if initial.is_some() {
        // The probe response always starts at byte 0, i.e. segment 0. Mark it
        // busy before any other worker can claim it.
        let mut t = shared.lock();
        t[0].active = true;
    }
    let mut set = JoinSet::new();
    let mut initial = initial;
    for w in 0..workers {
        let shared = shared.clone();
        let first = if w == 0 { initial.take() } else { None };
        set.spawn(async move { worker(shared, first).await });
    }

    // Periodic checkpoint: sync file, then persist the written counters.
    let checkpoint = {
        let shared = shared.clone();
        let base = d.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(CHECKPOINT_INTERVAL);
            interval.tick().await;
            loop {
                interval.tick().await;
                let segments = shared.snapshot();
                sync_file(&shared.file).await;
                let mut snap = base.clone();
                snap.downloaded = segments.iter().map(|s| s.written).sum();
                snap.segments = segments;
                (shared.ctx.checkpoint)(&snap);
            }
        })
    };

    let mut first_error: Option<DownloadError> = None;
    let mut stopped = false;
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(SegExit::Done)) => {}
            Ok(Ok(SegExit::Stopped)) => stopped = true,
            Ok(Err(e)) => {
                if first_error.is_none() {
                    shared.abort.store(true, Ordering::SeqCst);
                    first_error = Some(e);
                }
            }
            Err(join) => {
                shared.abort.store(true, Ordering::SeqCst);
                first_error.get_or_insert(DownloadError::internal(format!("download worker crashed: {join}")));
            }
        }
    }
    checkpoint.abort();
    ctx.progress.active_connections.store(0, Ordering::Relaxed);
    d.segments = shared.snapshot();
    let all_done = shared.lock().iter().all(|s| s.done || s.state.is_complete());

    if let Some(e) = first_error {
        return Err(e);
    }
    if stopped || *ctx.stop.borrow() != StopSignal::Run {
        return Ok(SegmentsResult::Stopped);
    }
    if all_done {
        Ok(SegmentsResult::Done)
    } else {
        Err(DownloadError::new(ErrorCode::SizeMismatch, "Download ended before all data was received"))
    }
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<SegRt>> {
        self.table.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn snapshot(&self) -> Vec<SegmentState> {
        let mut segs: Vec<SegmentState> = self.lock().iter().map(|s| s.state).collect();
        segs.sort_by_key(|s| s.start);
        segs
    }

    fn stopped(&self, stop: &watch::Receiver<StopSignal>) -> bool {
        *stop.borrow() != StopSignal::Run || self.abort.load(Ordering::SeqCst)
    }

    /// Claim an idle incomplete segment, or split the largest remaining
    /// range of an active one (work stealing). Returns the segment index.
    fn claim(&self) -> Option<usize> {
        let mut t = self.lock();
        if let Some(i) = t.iter().position(|s| !s.active && !s.done && !s.state.is_complete()) {
            t[i].active = true;
            t[i].received = t[i].state.written;
            return Some(i);
        }
        if !self.resumable || self.total.is_none() {
            return None;
        }
        let min = self.ctx.min_segment_size;
        let (idx, remaining) = t
            .iter()
            .enumerate()
            .filter(|(_, s)| s.active)
            .filter_map(|(i, s)| {
                let end = s.state.end?;
                let pos = s.state.start + s.received;
                (end + 1 > pos).then(|| (i, end + 1 - pos))
            })
            .max_by_key(|(_, r)| *r)?;
        if remaining < 2 * min {
            return None;
        }
        let seg = t[idx];
        let pos = seg.state.start + seg.received;
        let old_end = seg.state.end.expect("filtered above");
        let mid = pos + remaining / 2;
        t[idx].state.end = Some(mid - 1);
        t.push(SegRt {
            state: SegmentState { start: mid, end: Some(old_end), written: 0 },
            received: 0,
            active: true,
            done: false,
        });
        Some(t.len() - 1)
    }

    fn release(&self, idx: usize) {
        self.lock()[idx].active = false;
    }
}

async fn worker(shared: Arc<Shared>, mut initial: Option<InitialResponse>) -> Result<SegExit> {
    let mut stop = shared.ctx.stop.clone();
    loop {
        if shared.stopped(&stop) {
            return Ok(SegExit::Stopped);
        }
        let idx = if initial.is_some() {
            0
        } else {
            match shared.claim() {
                Some(i) => i,
                None => return Ok(SegExit::Done),
            }
        };
        let res = download_segment(&shared, idx, initial.take(), &mut stop).await;
        shared.release(idx);
        match res {
            Ok(SegExit::Done) => continue,
            other => return other,
        }
    }
}

async fn download_segment(
    shared: &Arc<Shared>,
    idx: usize,
    mut initial: Option<InitialResponse>,
    stop: &mut watch::Receiver<StopSignal>,
) -> Result<SegExit> {
    let ctx = &shared.ctx;
    let mut attempts = 0u32;
    loop {
        if shared.stopped(stop) {
            return Ok(SegExit::Stopped);
        }
        let (start, written, end) = {
            let mut t = shared.lock();
            let s = &mut t[idx];
            // Unwritten buffered data was dropped by a failed attempt.
            s.received = s.state.written;
            (s.state.start, s.state.written, s.state.end)
        };
        if end.map(|e| start + written > e).unwrap_or(false) {
            return Ok(SegExit::Done);
        }
        let mut pos = start + written;
        if pos > 0 && !shared.resumable {
            // Non-resumable stream broke off: start over from byte 0.
            let mut t = shared.lock();
            t[idx].state.written = 0;
            t[idx].received = 0;
            ctx.progress.received.store(0, Ordering::Relaxed);
            pos = 0;
        }

        let (mut resp, meta, _permit) = match initial.take() {
            Some(i) => (i.resp, i.meta, i.permit),
            None => {
                let Some(permit) = acquire_permit(ctx, stop).await? else { return Ok(SegExit::Stopped) };
                let range = if shared.resumable { Some((pos, end)) } else { None };
                let spec = RequestSpec {
                    url: &shared.url,
                    referrer: shared.referrer.as_deref(),
                    auth: Some(&ctx.auth),
                    range,
                    if_range: if shared.resumable { shared.if_range.as_deref() } else { None },
                };
                let sent = tokio::select! {
                    r = http::send(&ctx.client, &spec) => r,
                    _ = wait_stop(stop) => return Ok(SegExit::Stopped),
                };
                match sent.and_then(|(resp, meta)| validate_segment_response(shared, &meta, pos, range.is_some()).map(|_| (resp, meta))) {
                    Ok((resp, meta)) => (resp, meta, permit),
                    Err(e) => {
                        drop(permit);
                        if e.retryable && attempts < ctx.max_retries {
                            attempts += 1;
                            let delay = e.retry_after.unwrap_or_else(|| backoff(attempts));
                            tracing::debug!(segment = idx, attempts, error = %e.message, "segment request failed, retrying");
                            tokio::select! {
                                _ = tokio::time::sleep(delay) => continue,
                                _ = wait_stop(stop) => return Ok(SegExit::Stopped),
                            }
                        }
                        return Err(e);
                    }
                }
            }
        };

        ctx.progress.active_connections.fetch_add(1, Ordering::Relaxed);
        let streamed = stream_body(shared, idx, &mut resp, stop).await;
        ctx.progress.active_connections.fetch_sub(1, Ordering::Relaxed);
        let response_end = meta.content_range.map(|r| r.end);

        match streamed {
            Ok(StreamEnd::SegmentComplete) => return Ok(SegExit::Done),
            Ok(StreamEnd::Stopped) => return Ok(SegExit::Stopped),
            Ok(StreamEnd::Eof { made_progress }) => {
                let (finished, pos_now) = {
                    let mut t = shared.lock();
                    let s = &mut t[idx];
                    let pos_now = s.state.start + s.state.written;
                    if s.state.end.is_none() {
                        // Unknown length: the end of the stream is the end of the file.
                        if s.state.written > 0 {
                            s.state.end = Some(s.state.start + s.state.written - 1);
                        }
                        s.done = true;
                        (true, pos_now)
                    } else {
                        (s.state.is_complete(), pos_now)
                    }
                };
                if finished {
                    return Ok(SegExit::Done);
                }
                // The server may cap the size of a range response; that is
                // not an error, just ask for the rest.
                let capped = response_end.map(|e| pos_now > e).unwrap_or(false);
                if made_progress {
                    attempts = 0;
                }
                if !capped {
                    if attempts >= ctx.max_retries {
                        return Err(DownloadError::new(
                            ErrorCode::Network,
                            "The server closed the connection before the download was complete",
                        ));
                    }
                    attempts += 1;
                    tokio::select! {
                        _ = tokio::time::sleep(backoff(attempts)) => {}
                        _ = wait_stop(stop) => return Ok(SegExit::Stopped),
                    }
                }
                continue;
            }
            Err((e, made_progress)) => {
                if made_progress {
                    attempts = 0;
                }
                if e.retryable && attempts < ctx.max_retries {
                    attempts += 1;
                    let delay = e.retry_after.unwrap_or_else(|| backoff(attempts));
                    tracing::debug!(segment = idx, attempts, error = %e.message, "transfer interrupted, retrying");
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => continue,
                        _ = wait_stop(stop) => return Ok(SegExit::Stopped),
                    }
                }
                return Err(e);
            }
        }
    }
}

/// Check that a response really is the data we asked for.
fn validate_segment_response(shared: &Shared, meta: &ResponseMeta, pos: u64, ranged: bool) -> Result<()> {
    let changed = |why: &str| {
        Err(DownloadError::new(ErrorCode::ResourceChanged, format!("The file on the server changed ({why})")))
    };
    if ranged {
        if meta.status != 206 {
            // If-Range mismatch or ranges no longer supported.
            if pos == 0 && meta.status == 200 && meta.content_length == shared.total {
                return Ok(());
            }
            return changed("range request answered with full content");
        }
        let Some(cr) = meta.content_range else { return changed("missing Content-Range") };
        if cr.start != pos {
            return changed("unexpected range start");
        }
        if cr.total.is_some() && cr.total != shared.total {
            return changed("different size");
        }
    }
    if let (Some(old), Some(new)) = (shared.etag.as_deref(), meta.etag.as_deref()) {
        // Compare ignoring the weak prefix: some servers add/remove W/ per request.
        if old.trim_start_matches("W/") != new.trim_start_matches("W/") {
            return changed("ETag changed");
        }
    }
    Ok(())
}

enum StreamEnd {
    SegmentComplete,
    Stopped,
    Eof { made_progress: bool },
}

async fn stream_body(
    shared: &Arc<Shared>,
    idx: usize,
    resp: &mut reqwest::Response,
    stop: &mut watch::Receiver<StopSignal>,
) -> std::result::Result<StreamEnd, (DownloadError, bool)> {
    let ctx = &shared.ctx;
    let mut buf: Vec<u8> = Vec::with_capacity(FLUSH_SIZE);
    let mut made_progress = false;
    ctx.progress.phase.store(PHASE_DOWNLOADING, Ordering::Relaxed);
    loop {
        let chunk = tokio::select! {
            c = resp.chunk() => c,
            _ = wait_stop(stop) => {
                flush(shared, idx, &mut buf).await.map_err(|e| (e, made_progress))?;
                return Ok(StreamEnd::Stopped);
            }
        };
        if shared.abort.load(Ordering::SeqCst) {
            flush(shared, idx, &mut buf).await.map_err(|e| (e, made_progress))?;
            return Ok(StreamEnd::Stopped);
        }
        match chunk {
            Ok(Some(bytes)) => {
                let (take, complete) = {
                    let mut t = shared.lock();
                    let s = &mut t[idx];
                    let pos = s.state.start + s.received;
                    let allowed = match s.state.end {
                        Some(end) => (end + 1).saturating_sub(pos),
                        None => u64::MAX,
                    };
                    let take = (bytes.len() as u64).min(allowed);
                    s.received += take;
                    let complete = s.state.end.map(|e| s.state.start + s.received > e).unwrap_or(false);
                    (take as usize, complete)
                };
                if take > 0 {
                    made_progress = true;
                    buf.extend_from_slice(&bytes[..take]);
                    ctx.progress.received.fetch_add(take as u64, Ordering::Relaxed);
                    ctx.limiter.acquire(take as u64).await;
                }
                if complete {
                    flush(shared, idx, &mut buf).await.map_err(|e| (e, made_progress))?;
                    return Ok(StreamEnd::SegmentComplete);
                }
                if buf.len() >= FLUSH_SIZE {
                    flush(shared, idx, &mut buf).await.map_err(|e| (e, made_progress))?;
                }
            }
            Ok(None) => {
                flush(shared, idx, &mut buf).await.map_err(|e| (e, made_progress))?;
                return Ok(StreamEnd::Eof { made_progress });
            }
            Err(e) => {
                let flushed = flush(shared, idx, &mut buf).await;
                if let Err(fe) = flushed {
                    return Err((fe, made_progress));
                }
                let mut err = DownloadError::from(e);
                // A broken body is a network problem, not a request problem.
                if err.code == ErrorCode::InvalidUrl {
                    err = DownloadError::new(ErrorCode::Network, err.message);
                }
                return Err((err, made_progress));
            }
        }
    }
}

/// Write buffered bytes at the segment's write position.
async fn flush(shared: &Arc<Shared>, idx: usize, buf: &mut Vec<u8>) -> Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    let offset = {
        let t = shared.lock();
        t[idx].state.start + t[idx].state.written
    };
    let data = std::mem::replace(buf, Vec::with_capacity(FLUSH_SIZE));
    let len = data.len() as u64;
    let file = shared.file.clone();
    tokio::task::spawn_blocking(move || write_all_at(&file, &data, offset))
        .await
        .map_err(|e| DownloadError::internal(format!("write task failed: {e}")))?
        .map_err(|e| DownloadError::io(&e, "Cannot write to file"))?;
    shared.lock()[idx].state.written += len;
    Ok(())
}

#[cfg(unix)]
fn write_all_at(file: &File, data: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(data, offset)
}

#[cfg(windows)]
fn write_all_at(file: &File, mut data: &[u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !data.is_empty() {
        let n = file.seek_write(data, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "failed to write whole buffer"));
        }
        data = &data[n..];
        offset += n as u64;
    }
    Ok(())
}

async fn sync_file(file: &Arc<File>) {
    let f = file.clone();
    let _ = tokio::task::spawn_blocking(move || f.sync_data()).await;
}

/// Verify size and checksum, then atomically move the part file into place.
async fn finalize(ctx: &TaskContext, d: &mut Download, file: &Arc<File>) -> Result<Outcome> {
    let written: u64 = d.segments.iter().map(|s| s.written).sum();
    let total = match d.total_size {
        Some(t) => {
            if written != t {
                return Err(DownloadError::new(
                    ErrorCode::SizeMismatch,
                    format!("Incomplete download: {written} of {t} bytes received"),
                ));
            }
            t
        }
        None => written,
    };
    let f = file.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        f.set_len(total)?;
        f.sync_all()
    })
    .await
    .map_err(|e| DownloadError::internal(e.to_string()))?
    .map_err(|e| DownloadError::io(&e, "Cannot finalize file"))?;
    let actual = file.metadata().map_err(|e| DownloadError::io(&e, "Cannot read file"))?.len();
    if actual != total {
        return Err(DownloadError::new(ErrorCode::SizeMismatch, "File size on disk does not match"));
    }
    d.total_size = Some(total);
    d.downloaded = total;

    let part = d.part_path();
    if let Some(expected) = d.expected_sha256.clone() {
        let path = part.clone();
        let digest = tokio::task::spawn_blocking(move || sha256_file(&path))
            .await
            .map_err(|e| DownloadError::internal(e.to_string()))?
            .map_err(|e| DownloadError::io(&e, "Cannot verify file"))?;
        if !digest.eq_ignore_ascii_case(expected.trim()) {
            return Err(DownloadError::new(
                ErrorCode::ChecksumMismatch,
                "SHA-256 checksum does not match, the file is corrupted or was modified",
            )
            .retryable(false));
        }
    }

    let mut target = d.target_path();
    if target.exists() {
        d.file_name = (ctx.unique_name)(&d.save_dir, &d.file_name);
        target = d.target_path();
    }
    std::fs::rename(&part, &target).map_err(|e| DownloadError::io(&e, "Cannot move finished file into place"))?;
    Ok(Outcome::Completed { path: target })
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_segments() {
        let mb = 1024 * 1024;
        let s = plan_segments(Some(10 * mb), true, 4, mb);
        assert_eq!(s.len(), 4);
        assert_eq!(s[0].start, 0);
        assert_eq!(s.last().unwrap().end, Some(10 * mb - 1));
        for w in s.windows(2) {
            assert_eq!(w[0].end.unwrap() + 1, w[1].start);
        }
        assert_eq!(plan_segments(Some(10 * mb), false, 4, mb).len(), 1);
        assert_eq!(plan_segments(Some(mb / 2), true, 8, mb).len(), 1);
        assert_eq!(plan_segments(None, true, 8, mb), vec![SegmentState { start: 0, end: None, written: 0 }]);
        assert!(plan_segments(Some(0), true, 8, mb).is_empty());
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert!(backoff(1) <= Duration::from_millis(1200));
        assert!(backoff(3) >= Duration::from_millis(3200));
        assert!(backoff(30) <= Duration::from_millis(72_000));
    }
}
