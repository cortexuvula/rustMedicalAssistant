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

// ─────────────────────────────────────────────────────────────────────────────
// Retry classification
// ─────────────────────────────────────────────────────────────────────────────

/// The classification of a single request outcome for retry decisions.
#[derive(Debug, Clone, PartialEq)]
pub enum RetryDecision {
    /// 2xx — done.
    Success,
    /// Non-retryable error — return immediately.
    Permanent,
    /// Retryable — use the configured backoff schedule.
    Transient,
    /// Retryable with a server-specified delay (from `Retry-After`).
    TransientWithDelay(Duration),
}

/// Parse a `Retry-After` HTTP header.
/// Supports `delta-seconds` (RFC 7231 § 7.1.3). Returns `None` if absent,
/// not a valid integer, or HTTP-date format (HTTP-date is intentionally
/// unsupported — local providers do not send it).
pub fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let v = headers.get(reqwest::header::RETRY_AFTER)?;
    let s = v.to_str().ok()?;
    s.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// Classify an HTTP status (with response headers) for retry purposes.
pub fn classify_status(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> RetryDecision {
    if status.is_success() {
        return RetryDecision::Success;
    }
    let transient = matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504);
    if !transient {
        return RetryDecision::Permanent;
    }
    if let Some(d) = parse_retry_after(headers) {
        RetryDecision::TransientWithDelay(d)
    } else {
        RetryDecision::Transient
    }
}

/// Classify a transport-level error from `reqwest`.
///
/// Connection-refused (`is_connect()`) is treated as **Permanent** — the
/// local provider isn't running, and retrying for 7 s won't change that.
/// Read/connect timeouts and other transport errors are **Transient**.
/// Body/decode errors mean the server returned malformed data — Permanent.
pub fn classify_error(err: &reqwest::Error) -> RetryDecision {
    if err.is_connect() {
        return RetryDecision::Permanent;
    }
    if err.is_timeout() {
        return RetryDecision::Transient;
    }
    if err.is_body() || err.is_decode() {
        return RetryDecision::Permanent;
    }
    if err.is_request() {
        return RetryDecision::Transient;
    }
    RetryDecision::Permanent
}

/// Classify a `Result<reqwest::Response, reqwest::Error>` for retry purposes.
pub fn classify(
    result: &Result<reqwest::Response, reqwest::Error>,
) -> RetryDecision {
    match result {
        Ok(r) => classify_status(r.status(), r.headers()),
        Err(e) => classify_error(e),
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

    fn make_headers(retry_after: Option<&str>) -> reqwest::header::HeaderMap {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut h = HeaderMap::new();
        if let Some(v) = retry_after {
            h.insert(
                reqwest::header::RETRY_AFTER,
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn parse_retry_after_seconds() {
        let h = make_headers(Some("30"));
        assert_eq!(parse_retry_after(&h), Some(Duration::from_secs(30)));
    }

    #[test]
    fn parse_retry_after_missing_returns_none() {
        let h = make_headers(None);
        assert_eq!(parse_retry_after(&h), None);
    }

    #[test]
    fn parse_retry_after_malformed_returns_none() {
        let h = make_headers(Some("banana"));
        assert_eq!(parse_retry_after(&h), None);
    }

    #[test]
    fn parse_retry_after_unparseable_http_date_returns_none() {
        // HTTP-date support is deliberately not implemented (see plan scope reduction).
        let h = make_headers(Some("Sun, 06 Nov 1994 08:49:37 GMT"));
        assert_eq!(parse_retry_after(&h), None);
    }

    #[test]
    fn parse_retry_after_zero_seconds() {
        let h = make_headers(Some("0"));
        assert_eq!(parse_retry_after(&h), Some(Duration::ZERO));
    }

    #[test]
    fn classify_status_2xx_success() {
        let h = make_headers(None);
        assert_eq!(
            classify_status(reqwest::StatusCode::OK, &h),
            RetryDecision::Success
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::CREATED, &h),
            RetryDecision::Success
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::NO_CONTENT, &h),
            RetryDecision::Success
        );
    }

    #[test]
    fn classify_status_503_transient() {
        let h = make_headers(None);
        assert_eq!(
            classify_status(reqwest::StatusCode::SERVICE_UNAVAILABLE, &h),
            RetryDecision::Transient
        );
    }

    #[test]
    fn classify_status_503_with_retry_after() {
        let h = make_headers(Some("5"));
        assert_eq!(
            classify_status(reqwest::StatusCode::SERVICE_UNAVAILABLE, &h),
            RetryDecision::TransientWithDelay(Duration::from_secs(5))
        );
    }

    #[test]
    fn classify_status_429_with_retry_after() {
        let h = make_headers(Some("2"));
        assert_eq!(
            classify_status(reqwest::StatusCode::TOO_MANY_REQUESTS, &h),
            RetryDecision::TransientWithDelay(Duration::from_secs(2))
        );
    }

    #[test]
    fn classify_status_408_transient() {
        let h = make_headers(None);
        assert_eq!(
            classify_status(reqwest::StatusCode::REQUEST_TIMEOUT, &h),
            RetryDecision::Transient
        );
    }

    #[test]
    fn classify_status_500_502_504_transient() {
        let h = make_headers(None);
        for code in [500u16, 502, 504] {
            let s = reqwest::StatusCode::from_u16(code).unwrap();
            assert_eq!(
                classify_status(s, &h),
                RetryDecision::Transient,
                "code {code}"
            );
        }
    }

    #[test]
    fn classify_status_permanent_4xx() {
        let h = make_headers(None);
        for code in [400u16, 401, 403, 404, 405, 409, 410, 413, 414, 415, 422] {
            let s = reqwest::StatusCode::from_u16(code).unwrap();
            assert_eq!(
                classify_status(s, &h),
                RetryDecision::Permanent,
                "code {code}"
            );
        }
    }

    #[test]
    fn classify_status_permanent_other_5xx() {
        let h = make_headers(None);
        for code in [501u16, 505] {
            let s = reqwest::StatusCode::from_u16(code).unwrap();
            assert_eq!(
                classify_status(s, &h),
                RetryDecision::Permanent,
                "code {code}"
            );
        }
    }
}
