//! Token-bucket rate limiter.

use std::time::Instant;

/// A token-bucket rate limiter.
///
/// Tokens are refilled continuously at `refill_rate` tokens per second (derived
/// from the per-minute capacity), up to `capacity`.
pub struct RateLimiter {
    capacity: u32,
    tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a limiter that allows `requests_per_minute` requests per minute.
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            capacity: requests_per_minute,
            tokens: requests_per_minute as f64,
            refill_rate: requests_per_minute as f64 / 60.0,
            last_refill: Instant::now(),
        }
    }

    /// Attempt to consume one token.
    ///
    /// Returns `true` if the request is allowed, `false` if the bucket is
    /// empty.
    pub fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Add tokens proportional to elapsed time since the last refill.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_within_capacity() {
        let mut rl = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(rl.try_acquire(), "should be allowed within capacity");
        }
    }

    #[test]
    fn blocks_when_exhausted() {
        let mut rl = RateLimiter::new(3);
        // Drain all tokens
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        assert!(rl.try_acquire());
        // Next request must be blocked
        assert!(!rl.try_acquire(), "should be blocked when exhausted");
    }
}
