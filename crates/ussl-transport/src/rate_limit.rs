//! Rate limiting using Token Bucket algorithm

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use parking_lot::Mutex;

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per second
    pub requests_per_second: u32,
    /// Burst capacity (max tokens)
    pub burst_size: u32,
}

impl RateLimitConfig {
    pub fn new(requests_per_second: u32, burst_size: u32) -> Self {
        Self {
            requests_per_second,
            burst_size,
        }
    }

    /// Create from a simple "requests/second" value with default burst = 2x rate
    pub fn from_rate(requests_per_second: u32) -> Self {
        Self {
            requests_per_second,
            burst_size: requests_per_second * 2,
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 1000,
            burst_size: 2000,
        }
    }
}

/// Token bucket rate limiter
///
/// Allows bursting up to `burst_size` requests, then limits to `requests_per_second`.
/// Tokens are refilled continuously at the configured rate.
pub struct RateLimiter {
    /// Current tokens (scaled by 1000 for precision)
    tokens: AtomicU64,
    /// Last refill time
    last_refill: Mutex<Instant>,
    /// Configuration
    config: RateLimitConfig,
    /// Tokens per millisecond (scaled by 1000)
    tokens_per_ms: u64,
    /// Max tokens (scaled by 1000)
    max_tokens: u64,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        // Scale by 1000 for sub-token precision
        let max_tokens = (config.burst_size as u64) * 1000;
        let tokens_per_ms = (config.requests_per_second as u64 * 1000) / 1000; // tokens per ms

        Self {
            tokens: AtomicU64::new(max_tokens),
            last_refill: Mutex::new(Instant::now()),
            config,
            tokens_per_ms,
            max_tokens,
        }
    }

    /// Try to acquire a token. Returns true if allowed, false if rate limited.
    pub fn try_acquire(&self) -> bool {
        self.refill();

        // Try to consume one token (1000 in scaled units)
        let cost = 1000u64;

        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current < cost {
                return false;
            }

            match self.tokens.compare_exchange_weak(
                current,
                current - cost,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(_) => continue, // Retry
            }
        }
    }

    /// Check if next request would be rate limited (without consuming a token)
    pub fn would_limit(&self) -> bool {
        self.refill();
        self.tokens.load(Ordering::Relaxed) < 1000
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let mut last = self.last_refill.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(*last);

        if elapsed.as_millis() > 0 {
            let new_tokens = self.tokens_per_ms * elapsed.as_millis() as u64;

            if new_tokens > 0 {
                *last = now;

                let current = self.tokens.load(Ordering::Relaxed);
                let new_total = (current + new_tokens).min(self.max_tokens);
                self.tokens.store(new_total, Ordering::Relaxed);
            }
        }
    }

    /// Get current available tokens
    pub fn available_tokens(&self) -> u32 {
        self.refill();
        (self.tokens.load(Ordering::Relaxed) / 1000) as u32
    }

    /// Get the rate limit configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Reset the rate limiter to full capacity
    pub fn reset(&self) {
        self.tokens.store(self.max_tokens, Ordering::Relaxed);
        *self.last_refill.lock() = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_basic_rate_limiting() {
        let limiter = RateLimiter::new(RateLimitConfig::new(10, 5));

        // Should allow burst up to burst_size
        for _ in 0..5 {
            assert!(limiter.try_acquire());
        }

        // 6th request should be limited
        assert!(!limiter.try_acquire());
    }

    #[test]
    fn test_token_refill() {
        let limiter = RateLimiter::new(RateLimitConfig::new(1000, 10));

        // Exhaust all tokens
        for _ in 0..10 {
            limiter.try_acquire();
        }
        assert!(!limiter.try_acquire());

        // Wait for refill (at 1000/s = 1 per ms)
        thread::sleep(Duration::from_millis(20));

        // Should have ~20 tokens now (capped at burst_size=10)
        assert!(limiter.try_acquire());
    }

    #[test]
    fn test_would_limit() {
        let limiter = RateLimiter::new(RateLimitConfig::new(10, 2));

        assert!(!limiter.would_limit());
        limiter.try_acquire();
        assert!(!limiter.would_limit());
        limiter.try_acquire();
        assert!(limiter.would_limit());
    }

    #[test]
    fn test_reset() {
        let limiter = RateLimiter::new(RateLimitConfig::new(10, 5));

        // Exhaust tokens
        for _ in 0..5 {
            limiter.try_acquire();
        }
        assert_eq!(limiter.available_tokens(), 0);

        // Reset
        limiter.reset();
        assert_eq!(limiter.available_tokens(), 5);
    }
}
