//! Shared HTTP client infrastructure with retry and circuit-breaker support.

use std::time::{Duration, Instant};
use reqwest::{Client, header};
use rand::Rng;

use medical_core::error::{AppError, AppResult};
use medical_core::types::settings::AppConfig;

/// Build a reqwest client with Bearer-token auth.
///
/// Returns `Err(AppError::AiProvider(...))` if the API key contains characters
/// that are invalid in HTTP header values (newlines, raw control bytes) or if
/// reqwest's builder fails — the caller decides how to surface that.
pub fn build_client(api_key: &str, timeout_secs: u64) -> AppResult<Client> {
    let mut auth_value = header::HeaderValue::from_str(&format!("Bearer {api_key}"))
        .map_err(|_| {
            AppError::AiProvider("API key contains characters invalid in HTTP headers".into())
        })?;
    auth_value.set_sensitive(true);

    let mut headers = header::HeaderMap::new();
    headers.insert(header::AUTHORIZATION, auth_value);

    Client::builder()
        .default_headers(headers)
        .pool_max_idle_per_host(5)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| AppError::AiProvider(format!("Failed to build HTTP client: {e}")))
}

/// Build a reqwest client with a custom auth header.
pub fn build_client_custom_auth(
    header_name: &str,
    api_key: &str,
    timeout_secs: u64,
) -> AppResult<Client> {
    let header_name = header::HeaderName::from_bytes(header_name.as_bytes())
        .map_err(|_| AppError::AiProvider(format!("Invalid auth header name: {header_name:?}")))?;

    let mut header_value = header::HeaderValue::from_str(api_key).map_err(|_| {
        AppError::AiProvider("API key contains characters invalid in HTTP headers".into())
    })?;
    header_value.set_sensitive(true);

    let mut headers = header::HeaderMap::new();
    headers.insert(header_name, header_value);

    Client::builder()
        .default_headers(headers)
        .pool_max_idle_per_host(5)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| AppError::AiProvider(format!("Failed to build HTTP client: {e}")))
}

/// Configuration for exponential-backoff retry logic.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub backoff_factor: f64,
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryConfig {
    /// Return the delay to wait before `attempt` (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let millis = self.initial_delay.as_millis() as f64
            * self.backoff_factor.powi(attempt as i32);
        let capped = millis.min(self.max_delay.as_millis() as f64) as u64;
        Duration::from_millis(capped)
    }

    /// Construct policy from user-facing AppConfig settings.
    /// `auto_retry_failed=false` produces `max_retries=0` (no retries).
    /// Tuning constants (initial_delay, backoff_factor, max_delay) stay at defaults.
    pub fn from_app_config(cfg: &AppConfig) -> Self {
        let default = Self::default();
        Self {
            max_retries: if cfg.auto_retry_failed {
                cfg.max_retry_attempts
            } else {
                0
            },
            ..default
        }
    }

    /// Apply ±25% jitter to a base delay.
    /// Returned duration ∈ [0.75 × base, 1.25 × base].
    /// Caller passes its own RNG so tests can use a seeded one.
    pub fn jittered<R: Rng + ?Sized>(&self, base: Duration, rng: &mut R) -> Duration {
        if base.is_zero() {
            return base;
        }
        let factor = rng.gen_range(0.75..=1.25);
        let millis = (base.as_millis() as f64 * factor) as u64;
        Duration::from_millis(millis)
    }
}

/// Simple circuit breaker.
#[derive(Debug)]
pub struct CircuitBreaker {
    pub failure_count: u32,
    pub failure_threshold: u32,
    pub last_failure: Option<Instant>,
    pub recovery_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_count: 0,
            failure_threshold,
            last_failure: None,
            recovery_timeout,
        }
    }

    /// Returns `true` when the breaker is open (circuit broken, reject requests).
    pub fn is_open(&self) -> bool {
        if self.failure_count < self.failure_threshold {
            return false;
        }
        match self.last_failure {
            None => false,
            Some(t) => t.elapsed() < self.recovery_timeout,
        }
    }

    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.last_failure = None;
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.delay_for_attempt(0), Duration::from_secs(1));
        assert_eq!(cfg.delay_for_attempt(1), Duration::from_secs(2));
        assert_eq!(cfg.delay_for_attempt(2), Duration::from_secs(4));
    }

    #[test]
    fn caps_at_max() {
        let cfg = RetryConfig::default();
        // attempt 10: 1 * 2^10 = 1024 s, capped at 30 s
        assert_eq!(cfg.delay_for_attempt(10), Duration::from_secs(30));
    }

    #[test]
    fn cb_starts_closed() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        assert!(!cb.is_open());
    }

    #[test]
    fn cb_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open());
    }

    #[test]
    fn cb_resets_on_success() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open());
        cb.record_success();
        assert!(!cb.is_open());
    }

    #[test]
    fn from_app_config_disabled_zero_retries() {
        use medical_core::types::settings::AppConfig;
        let mut cfg = AppConfig::default();
        cfg.auto_retry_failed = false;
        cfg.max_retry_attempts = 5;
        let policy = RetryConfig::from_app_config(&cfg);
        assert_eq!(policy.max_retries, 0);
    }

    #[test]
    fn from_app_config_reads_max_attempts() {
        use medical_core::types::settings::AppConfig;
        let mut cfg = AppConfig::default();
        cfg.auto_retry_failed = true;
        cfg.max_retry_attempts = 5;
        let policy = RetryConfig::from_app_config(&cfg);
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_delay, Duration::from_secs(1));
        assert!((policy.backoff_factor - 2.0).abs() < f64::EPSILON);
        assert_eq!(policy.max_delay, Duration::from_secs(30));
    }

    #[test]
    fn from_app_config_default_uses_three_retries() {
        use medical_core::types::settings::AppConfig;
        let cfg = AppConfig::default();
        let policy = RetryConfig::from_app_config(&cfg);
        // AppConfig defaults: auto_retry_failed=true, max_retry_attempts=3.
        assert_eq!(policy.max_retries, 3);
    }

    #[test]
    fn jittered_within_25_percent_band() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let cfg = RetryConfig::default();
        let base = Duration::from_millis(1000);
        let mut rng = StdRng::seed_from_u64(42);
        for i in 0..1000 {
            let j = cfg.jittered(base, &mut rng);
            assert!(
                j >= Duration::from_millis(750),
                "iter {i}: got {j:?}, expected >= 750ms"
            );
            assert!(
                j <= Duration::from_millis(1250),
                "iter {i}: got {j:?}, expected <= 1250ms"
            );
        }
    }

    #[test]
    fn jittered_zero_base_is_zero() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let cfg = RetryConfig::default();
        let mut rng = StdRng::seed_from_u64(0);
        assert_eq!(cfg.jittered(Duration::ZERO, &mut rng), Duration::ZERO);
    }

    #[test]
    fn jittered_distribution_spans_band() {
        // Sanity check: over many samples, both halves of the band are visited.
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let cfg = RetryConfig::default();
        let base = Duration::from_millis(1000);
        let mut rng = StdRng::seed_from_u64(123);
        let mut saw_below = false;
        let mut saw_above = false;
        for _ in 0..500 {
            let j = cfg.jittered(base, &mut rng);
            if j < base { saw_below = true; }
            if j > base { saw_above = true; }
        }
        assert!(saw_below, "expected at least one jittered sample < base");
        assert!(saw_above, "expected at least one jittered sample > base");
    }
}
