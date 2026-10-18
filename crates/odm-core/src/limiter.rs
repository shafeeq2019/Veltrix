//! Global bandwidth limiter (token bucket with debt).
//!
//! Every connection asks for the bytes it just received. When the bucket
//! is empty the caller goes into debt and sleeps for exactly the time the
//! debt needs to be repaid. Because each waiter adds to the debt, waiters
//! are served roughly in arrival order, which spreads the bandwidth fairly
//! across connections and downloads.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    /// Bytes per second; 0 = unlimited.
    rate: AtomicU64,
    bucket: Mutex<Bucket>,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(0)
    }
}

impl RateLimiter {
    pub fn new(rate: u64) -> Self {
        RateLimiter { rate: AtomicU64::new(rate), bucket: Mutex::new(Bucket { tokens: 0.0, last: Instant::now() }) }
    }

    pub fn set_rate(&self, rate: u64) {
        let old = self.rate.swap(rate, Ordering::Relaxed);
        if old != rate {
            let mut b = self.bucket.lock().unwrap_or_else(|p| p.into_inner());
            b.tokens = 0.0;
            b.last = Instant::now();
        }
    }

    pub fn rate(&self) -> u64 {
        self.rate.load(Ordering::Relaxed)
    }

    /// How long the caller has to wait after consuming `bytes`.
    pub fn reserve(&self, bytes: u64) -> Option<Duration> {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 || bytes == 0 {
            return None;
        }
        let rate_f = rate as f64;
        // Allow bursts of up to 1/4 s worth of data.
        let burst = (rate_f / 4.0).max(16.0 * 1024.0);
        let mut b = self.bucket.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let elapsed = now.duration_since(b.last).as_secs_f64();
        b.last = now;
        b.tokens = (b.tokens + elapsed * rate_f).min(burst);
        b.tokens -= bytes as f64;
        if b.tokens >= 0.0 {
            None
        } else {
            Some(Duration::from_secs_f64((-b.tokens / rate_f).min(30.0)))
        }
    }

    pub async fn acquire(&self, bytes: u64) {
        if let Some(wait) = self.reserve(bytes) {
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_never_waits() {
        let l = RateLimiter::new(0);
        assert!(l.reserve(u64::MAX / 2).is_none());
    }

    #[test]
    fn debt_produces_proportional_wait() {
        let l = RateLimiter::new(1000 * 1024);
        // first call: bucket empty, 512 KiB at 1000 KiB/s ≈ 0.5 s
        let w = l.reserve(512 * 1024).unwrap();
        assert!(w > Duration::from_millis(400) && w < Duration::from_millis(600), "{w:?}");
        // the next caller queues behind the first
        let w2 = l.reserve(512 * 1024).unwrap();
        assert!(w2 > w);
    }

    #[tokio::test]
    async fn throughput_is_capped() {
        let l = RateLimiter::new(1024 * 1024);
        let start = Instant::now();
        for _ in 0..20 {
            l.acquire(64 * 1024).await;
        }
        // 1.25 MiB at 1 MiB/s needs about 1.25 s.
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(1000), "{elapsed:?}");
        assert!(elapsed <= Duration::from_millis(2500), "{elapsed:?}");
    }
}
