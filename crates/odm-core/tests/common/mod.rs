//! Local HTTP test server with configurable misbehaviour.
//!
//! Routes (all content is deterministic, see [`byte_at`]):
//! * `/file/{size}`        ranges + ETag + Last-Modified
//! * `/norange/{size}`     ignores Range, always 200
//! * `/unknown/{size}`     no Content-Length, no ranges (chunked)
//! * `/flaky/{size}`       first 2 responses abort after 200 KiB
//! * `/capped/{size}`      ranges, but every response carries at most 64 KiB
//! * `/slow/{size}`        ranges, ~256 KiB/s per connection
//! * `/changing/{size}`    ETag + content change after the first response
//! * `/limited/{size}`     429 with Retry-After: 1 on the first request
//! * `/redirect/{size}`    302 → /file/{size}
//! * `/status/{code}`      returns that status
//! * `/disposition/{size}` Content-Disposition: attachment; filename="report final.pdf"

#![allow(dead_code)]

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn byte_at(i: u64, variant: u64) -> u8 {
    ((i.wrapping_mul(31).wrapping_add(7 + variant * 13)) % 251) as u8
}

pub fn expected(size: u64, variant: u64) -> Vec<u8> {
    (0..size).map(|i| byte_at(i, variant)).collect()
}

#[derive(Default)]
pub struct Stats {
    pub requests: AtomicUsize,
    pub range_requests: AtomicUsize,
    pub per_path: Mutex<HashMap<String, usize>>,
    pub max_concurrent: AtomicUsize,
    pub current: AtomicUsize,
}

impl Stats {
    fn hit(&self, path: &str) -> usize {
        let mut m = self.per_path.lock().unwrap();
        let c = m.entry(path.to_string()).or_insert(0);
        *c += 1;
        *c
    }
}

pub struct TestServer {
    pub addr: SocketAddr,
    pub stats: Arc<Stats>,
}

impl TestServer {
    pub async fn start() -> TestServer {
        let stats = Arc::new(Stats::default());
        let app = Router::new()
            .route("/{kind}/{arg}", get(handler))
            .with_state(stats.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        TestServer { addr, stats }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

struct Guard(Arc<Stats>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.current.fetch_sub(1, Ordering::SeqCst);
    }
}

fn parse_range(headers: &HeaderMap, size: u64) -> Option<(u64, u64)> {
    let v = headers.get("range")?.to_str().ok()?;
    let r = v.strip_prefix("bytes=")?;
    let (a, b) = r.split_once('-')?;
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() { size.saturating_sub(1) } else { b.parse::<u64>().ok()?.min(size.saturating_sub(1)) };
    Some((start, end))
}

struct Plan {
    size: u64,
    ranges: bool,
    content_length: bool,
    abort_after: Option<u64>,
    cap: Option<u64>,
    rate: Option<u64>,
    variant: u64,
    etag: String,
}

async fn handler(
    State(stats): State<Arc<Stats>>,
    Path((kind, arg)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response<Body> {
    stats.requests.fetch_add(1, Ordering::SeqCst);
    let path = format!("/{kind}/{arg}");
    let hit = stats.hit(&path);
    if kind == "status" {
        let code: u16 = arg.parse().unwrap_or(500);
        return Response::builder().status(code).header("retry-after", "1").body(Body::from("error")).unwrap();
    }
    let size: u64 = arg.parse().unwrap_or(0);
    if kind == "redirect" {
        return Response::builder()
            .status(302)
            .header("location", format!("/file/{size}"))
            .body(Body::empty())
            .unwrap();
    }
    if kind == "limited" && hit == 1 {
        return Response::builder().status(429).header("retry-after", "1").body(Body::empty()).unwrap();
    }
    let mut plan = Plan {
        size,
        ranges: true,
        content_length: true,
        abort_after: None,
        cap: None,
        rate: None,
        variant: 0,
        etag: "\"v1\"".into(),
    };
    match kind.as_str() {
        "file" | "redirect" | "disposition" | "limited" => {}
        "norange" => plan.ranges = false,
        "unknown" => {
            plan.ranges = false;
            plan.content_length = false;
        }
        "flaky" => {
            if hit <= 2 {
                plan.abort_after = Some(200 * 1024);
            }
        }
        "capped" => plan.cap = Some(64 * 1024),
        "slow" => plan.rate = Some(256 * 1024),
        "changing" => {
            if hit > 1 {
                plan.variant = 1;
                plan.etag = "\"v2\"".into();
            }
        }
        _ => return Response::builder().status(404).body(Body::empty()).unwrap(),
    }

    let mut builder = Response::builder().header("content-type", "application/octet-stream");
    if plan.ranges {
        builder = builder
            .header("accept-ranges", "bytes")
            .header("etag", plan.etag.clone())
            .header("last-modified", "Wed, 21 Oct 2015 07:28:00 GMT");
    }
    if kind == "disposition" {
        builder = builder.header("content-disposition", "attachment; filename=\"report final.pdf\"");
    }
    let mut start = 0;
    let mut end = size.saturating_sub(1);
    let mut status = StatusCode::OK;
    if plan.ranges && size > 0 {
        let if_range_ok = headers
            .get("if-range")
            .map(|v| v.to_str().unwrap_or("") == plan.etag || v.to_str().unwrap_or("").contains("2015"))
            .unwrap_or(true);
        if let (Some((s, e)), true) = (parse_range(&headers, size), if_range_ok) {
            if s >= size {
                return Response::builder()
                    .status(416)
                    .header("content-range", format!("bytes */{size}"))
                    .body(Body::empty())
                    .unwrap();
            }
            stats.range_requests.fetch_add(1, Ordering::SeqCst);
            start = s;
            end = e;
            if let Some(cap) = plan.cap {
                end = end.min(start + cap - 1);
            }
            status = StatusCode::PARTIAL_CONTENT;
            builder = builder.header("content-range", format!("bytes {start}-{end}/{size}"));
        }
    } else if plan.ranges && size == 0 && headers.get("range").is_some() {
        return Response::builder().status(416).header("content-range", "bytes */0").body(Body::empty()).unwrap();
    }
    let len = if size == 0 { 0 } else { end + 1 - start };
    if plan.content_length {
        builder = builder.header("content-length", HeaderValue::from(len));
    }
    let current = stats.current.fetch_add(1, Ordering::SeqCst) + 1;
    stats.max_concurrent.fetch_max(current, Ordering::SeqCst);
    let guard = Guard(stats.clone());

    let variant = plan.variant;
    let abort_after = plan.abort_after;
    let rate = plan.rate;
    let chunk = 16 * 1024u64;
    let stream = futures_util::stream::unfold((start, 0u64, guard), move |(pos, sent, guard)| async move {
        if len == 0 || pos > end {
            return None;
        }
        if let Some(limit) = abort_after {
            if sent >= limit {
                return Some((Err(std::io::Error::other("simulated connection abort")), (end + 1, sent, guard)));
            }
        }
        if let Some(rate) = rate {
            tokio::time::sleep(Duration::from_secs_f64(chunk as f64 / rate as f64)).await;
        }
        let n = chunk.min(end + 1 - pos);
        let data: Vec<u8> = (pos..pos + n).map(|i| byte_at(i, variant)).collect();
        Some((Ok::<Bytes, std::io::Error>(Bytes::from(data)), (pos + n, sent + n, guard)))
    });
    builder.status(status).body(Body::from_stream(stream)).unwrap()
}
