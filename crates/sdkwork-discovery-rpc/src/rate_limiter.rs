use std::time::Instant;
use tracing::warn;

#[derive(Debug)]
pub struct TokenBucketRateLimiter {
    capacity: u64,
    tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucketRateLimiter {
    pub fn new(capacity: u64, refill_rate_per_second: f64) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate: refill_rate_per_second,
            last_refill: Instant::now(),
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            warn!(capacity = self.capacity, "rate limit exceeded");
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_second: u64,
    pub burst_capacity: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_second: 1000,
            burst_capacity: 1000,
        }
    }
}
