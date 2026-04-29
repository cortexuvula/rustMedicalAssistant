# AI Provider Retry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `OpenAiCompatibleClient` HTTP calls through a retry helper so transient 5xx / 429 / network errors recover automatically instead of failing the SOAP generation outright.

**Architecture:** Add a `send_with_retry(policy, factory)` helper to `crates/ai-providers/src/http_client.rs` that retries on classified-transient outcomes with exponential backoff + jitter, honoring `Retry-After` capped at `max_delay`. The four call sites in `openai_compat.rs` (`list_models`, `complete`, `complete_stream`, `complete_with_tools`) replace their `.send().await` with the helper. Each `OpenAiCompatibleClient` carries its own `RetryConfig`, constructed via `RetryConfig::from_app_config(&AppConfig)` at provider registration time in `state.rs`. The two existing settings fields `auto_retry_failed` and `max_retry_attempts` are honored; tuning constants stay hardcoded.

**Tech Stack:** Rust, tokio, reqwest 0.12, wiremock 0.6 (dev), rand 0.8, tracing.

**Spec:** `docs/superpowers/specs/2026-04-29-ai-provider-retry-design.md`

**Scope reduction (deliberate, vs. spec):** `parse_retry_after` only parses `delta-seconds` (numeric). HTTP-date parsing (RFC 1123) is dropped — the spec marked it "best-effort" and local providers (Ollama, LM Studio) do not send HTTP-date Retry-After. Returns `None` for HTTP-date values, which falls back to the configured backoff schedule. This avoids adding `httpdate` as a dependency.

---

## File Structure

| File | Responsibility | Status |
|------|----------------|--------|
| `crates/ai-providers/src/http_client.rs` | Retry policy types + retry/jitter/classify helpers + `send_with_retry`. Single source of truth for retry behavior. | Modified — adds ~150 LOC |
| `crates/ai-providers/src/openai_compat.rs` | OpenAI-compat client; the 4 call sites now go through `send_with_retry`. Holds a `RetryConfig` field. | Modified — ~25 LOC net change + Layer 3 tests |
| `crates/ai-providers/src/ollama.rs` | Ollama provider; pass `RetryConfig` through to the inner `OpenAiCompatibleClient`. | Modified — constructor signature change |
| `crates/ai-providers/src/lmstudio.rs` | Same as ollama.rs for LM Studio. | Modified — constructor signature change |
| `crates/ai-providers/Cargo.toml` | Add `rand = "0.8"` to deps; `wiremock = "0.6"` to dev-deps. | Modified |
| `src-tauri/src/state.rs` | `init_ai_providers` builds `RetryConfig::from_app_config(config)` once and passes it to both provider constructors. | Modified — ~5 LOC |

No new files. `crates/core/src/types/settings.rs` is **unchanged** — `auto_retry_failed` and `max_retry_attempts` already exist.

---

## Task 1: Add `rand` dependency and `RetryConfig::from_app_config`

**Files:**
- Modify: `crates/ai-providers/Cargo.toml`
- Modify: `crates/ai-providers/src/http_client.rs`

- [ ] **Step 1.1: Add `rand` dependency**

Edit `crates/ai-providers/Cargo.toml`. Under `[dependencies]`, add:

```toml
rand = "0.8"
```

Final dependency block:

```toml
[dependencies]
medical-core = { path = "../core" }
reqwest = { workspace = true }
eventsource-stream = { workspace = true }
tokio-stream = { workspace = true }
bytes = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
futures-core = { workspace = true }
futures-util = { workspace = true }
rand = "0.8"
```

- [ ] **Step 1.2: Verify the workspace builds with the new dep**

Run: `cargo build -p medical-ai-providers`
Expected: builds successfully.

- [ ] **Step 1.3: Add the failing tests for `RetryConfig::from_app_config`**

Edit `crates/ai-providers/src/http_client.rs`. Inside the existing `#[cfg(test)] mod tests { ... }` block, after the existing `caps_at_max` test, add:

```rust
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
```

- [ ] **Step 1.4: Run tests, verify they fail**

Run: `cargo test -p medical-ai-providers from_app_config -- --nocapture`
Expected: FAIL with `cannot find function 'from_app_config' in 'RetryConfig'`.

- [ ] **Step 1.5: Implement `from_app_config`**

Edit `crates/ai-providers/src/http_client.rs`. At the top of the file, change:

```rust
use medical_core::error::{AppError, AppResult};
```

to:

```rust
use medical_core::error::{AppError, AppResult};
use medical_core::types::settings::AppConfig;
```

Then, inside the existing `impl RetryConfig { ... }` block (the one with `delay_for_attempt`), add a new method:

```rust
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
```

- [ ] **Step 1.6: Run tests, verify they pass**

Run: `cargo test -p medical-ai-providers from_app_config`
Expected: 3 tests pass.

- [ ] **Step 1.7: Commit**

```bash
git add crates/ai-providers/Cargo.toml crates/ai-providers/src/http_client.rs
git commit -m "$(cat <<'EOF'
feat(ai-providers): add RetryConfig::from_app_config

Reads auto_retry_failed and max_retry_attempts from AppConfig.
First step in wiring the existing-but-unused retry policy through
the provider call sites.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add `RetryConfig::jittered` for ±25% delay jitter

**Files:**
- Modify: `crates/ai-providers/src/http_client.rs`

- [ ] **Step 2.1: Write failing tests**

Edit `crates/ai-providers/src/http_client.rs`. Inside `mod tests`, after `from_app_config_default_uses_three_retries`, add:

```rust
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
```

- [ ] **Step 2.2: Run tests, verify they fail**

Run: `cargo test -p medical-ai-providers jittered`
Expected: FAIL with `no method named 'jittered' found`.

- [ ] **Step 2.3: Implement `jittered`**

Edit `crates/ai-providers/src/http_client.rs`. At the top of the file, add the import:

```rust
use rand::Rng;
```

Inside the existing `impl RetryConfig { ... }` block, add:

```rust
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
```

- [ ] **Step 2.4: Run tests, verify they pass**

Run: `cargo test -p medical-ai-providers jittered`
Expected: 3 tests pass.

- [ ] **Step 2.5: Commit**

```bash
git add crates/ai-providers/src/http_client.rs
git commit -m "$(cat <<'EOF'
feat(ai-providers): add RetryConfig::jittered for ±25% delay jitter

Pure function that takes its own RNG so tests are deterministic.
Used by send_with_retry to space out retry attempts.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Add `RetryDecision`, `parse_retry_after`, and `classify`

**Files:**
- Modify: `crates/ai-providers/src/http_client.rs`

- [ ] **Step 3.1: Write failing tests**

Edit `crates/ai-providers/src/http_client.rs`. Inside `mod tests`, append:

```rust
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
```

- [ ] **Step 3.2: Run tests, verify they fail**

Run: `cargo test -p medical-ai-providers parse_retry_after classify_status`
Expected: FAIL — symbols not yet defined.

- [ ] **Step 3.3: Implement `RetryDecision`, `parse_retry_after`, `classify_status`, `classify_error`, `classify`**

Edit `crates/ai-providers/src/http_client.rs`. Insert the following block after the existing `CircuitBreaker` impl block and before the `#[cfg(test)] mod tests` block:

```rust
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
```

- [ ] **Step 3.4: Run tests, verify they pass**

Run: `cargo test -p medical-ai-providers parse_retry_after classify_status`
Expected: all 13 tests pass.

- [ ] **Step 3.5: Run full http_client test set as a sanity check**

Run: `cargo test -p medical-ai-providers --lib http_client`
Expected: previously passing tests still pass; new ones pass.

- [ ] **Step 3.6: Commit**

```bash
git add crates/ai-providers/src/http_client.rs
git commit -m "$(cat <<'EOF'
feat(ai-providers): add RetryDecision, parse_retry_after, classify

Pure functions that decide whether a request outcome is retryable.
classify_error tested indirectly via wiremock in the next task —
constructing reqwest::Error in unit tests is impractical.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Add `wiremock` dev-dep and implement `send_with_retry`

**Files:**
- Modify: `crates/ai-providers/Cargo.toml`
- Modify: `crates/ai-providers/src/http_client.rs`

- [ ] **Step 4.1: Add wiremock as dev-dependency**

Edit `crates/ai-providers/Cargo.toml`. Under `[dev-dependencies]`, add:

```toml
wiremock = "0.6"
```

Final `[dev-dependencies]` block:

```toml
[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
wiremock = "0.6"
```

- [ ] **Step 4.2: Verify the workspace builds**

Run: `cargo build -p medical-ai-providers --tests`
Expected: builds successfully.

- [ ] **Step 4.3: Write failing tests for `send_with_retry`**

Edit `crates/ai-providers/src/http_client.rs`. Inside `mod tests`, append:

```rust
    fn fast_policy(max_retries: u32) -> RetryConfig {
        // Tighter delays so tests don't take forever; same algorithm.
        RetryConfig {
            max_retries,
            initial_delay: Duration::from_millis(20),
            backoff_factor: 2.0,
            max_delay: Duration::from_millis(200),
        }
    }

    fn build_test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("test client")
    }

    #[tokio::test]
    async fn send_with_retry_succeeds_first_try() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = build_test_client();
        let url = format!("{}/v1/chat", server.uri());
        let policy = fast_policy(3);

        let resp = send_with_retry(&policy, || client.post(&url).body("hi"))
            .await
            .expect("ok");
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test]
    async fn send_with_retry_retries_on_503_then_succeeds() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = build_test_client();
        let url = format!("{}/v1/chat", server.uri());
        let policy = fast_policy(3);

        let resp = send_with_retry(&policy, || client.post(&url).body("hi"))
            .await
            .expect("ok");
        assert_eq!(resp.status(), 200);
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn send_with_retry_gives_up_after_max_attempts() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = build_test_client();
        let url = format!("{}/v1/chat", server.uri());
        let policy = fast_policy(3);

        let resp = send_with_retry(&policy, || client.post(&url).body("hi"))
            .await
            .expect("ok (final response is the 503)");
        assert_eq!(resp.status(), 503);
        // Initial + 3 retries = 4 requests.
        assert_eq!(server.received_requests().await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn send_with_retry_does_not_retry_400() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;

        let client = build_test_client();
        let url = format!("{}/v1/chat", server.uri());
        let policy = fast_policy(3);

        let resp = send_with_retry(&policy, || client.post(&url).body("hi"))
            .await
            .expect("ok");
        assert_eq!(resp.status(), 400);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn send_with_retry_does_not_retry_when_disabled() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = build_test_client();
        let url = format!("{}/v1/chat", server.uri());
        let policy = fast_policy(0); // disabled

        let resp = send_with_retry(&policy, || client.post(&url).body("hi"))
            .await
            .expect("ok");
        assert_eq!(resp.status(), 503);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn send_with_retry_honors_retry_after_header() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "1"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = build_test_client();
        let url = format!("{}/v1/chat", server.uri());
        let policy = fast_policy(3);

        let start = std::time::Instant::now();
        let resp = send_with_retry(&policy, || client.post(&url).body("hi"))
            .await
            .expect("ok");
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), 200);
        assert!(
            elapsed >= Duration::from_millis(900),
            "expected ≥ ~1 s wait honoring Retry-After: 1, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn send_with_retry_caps_retry_after_at_max_delay() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "9999"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        // policy.max_delay == 200 ms; the 9999s Retry-After must be capped.
        let client = build_test_client();
        let url = format!("{}/v1/chat", server.uri());
        let policy = fast_policy(3);

        let start = std::time::Instant::now();
        let resp = send_with_retry(&policy, || client.post(&url).body("hi"))
            .await
            .expect("ok");
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), 200);
        assert!(
            elapsed <= Duration::from_secs(2),
            "should cap Retry-After at policy.max_delay; got {elapsed:?}"
        );
    }
```

- [ ] **Step 4.4: Run tests, verify they fail**

Run: `cargo test -p medical-ai-providers send_with_retry`
Expected: FAIL — `cannot find function 'send_with_retry'`.

- [ ] **Step 4.5: Implement `send_with_retry`**

Edit `crates/ai-providers/src/http_client.rs`. Append (after the `classify` function added in Task 3, still before `mod tests`):

```rust
/// Send a request with retry/backoff per the configured policy.
///
/// `factory` is invoked fresh on each attempt because `reqwest::RequestBuilder`
/// is consumed by `.send()`. The closure typically captures `&Client` and a
/// `&str` URL plus a serializable body.
///
/// Returns the final `Result<Response, Error>` once the request succeeds, hits
/// a permanent classification, or runs out of retry budget.
pub async fn send_with_retry<F>(
    policy: &RetryConfig,
    factory: F,
) -> Result<reqwest::Response, reqwest::Error>
where
    F: Fn() -> reqwest::RequestBuilder + Send,
{
    use rand::thread_rng;

    let mut attempt: u32 = 0;
    loop {
        let result = factory().send().await;
        let decision = classify(&result);
        let delay = match decision {
            RetryDecision::Success => {
                if attempt > 0 {
                    tracing::info!(
                        attempts = attempt + 1,
                        "AI provider recovered after retries",
                    );
                }
                return result;
            }
            RetryDecision::Permanent => return result,
            RetryDecision::Transient => {
                if attempt >= policy.max_retries {
                    return result;
                }
                policy.jittered(policy.delay_for_attempt(attempt), &mut thread_rng())
            }
            RetryDecision::TransientWithDelay(server_delay) => {
                if attempt >= policy.max_retries {
                    return result;
                }
                std::cmp::min(server_delay, policy.max_delay)
            }
        };

        let status = result
            .as_ref()
            .ok()
            .map(|r| r.status().as_u16())
            .unwrap_or(0);
        tracing::info!(
            attempt = attempt + 1,
            max = policy.max_retries + 1,
            delay_ms = delay.as_millis() as u64,
            status,
            "AI provider transient failure, retrying",
        );
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}
```

- [ ] **Step 4.6: Run tests, verify they pass**

Run: `cargo test -p medical-ai-providers send_with_retry`
Expected: 7 tests pass. Total runtime ≤ ~5 s.

- [ ] **Step 4.7: Run full http_client test set**

Run: `cargo test -p medical-ai-providers --lib http_client`
Expected: all tests still pass.

- [ ] **Step 4.8: Commit**

```bash
git add crates/ai-providers/Cargo.toml crates/ai-providers/src/http_client.rs
git commit -m "$(cat <<'EOF'
feat(ai-providers): add send_with_retry helper

Retries transient HTTP failures with exponential backoff + ±25% jitter,
honoring Retry-After (capped at max_delay). Wiremock-based integration
tests cover the happy path, exhaustion, fast-fail on 4xx, disabled
policy, and Retry-After honoring/capping.

Closure-based factory parameter rebuilds RequestBuilder each attempt
since RequestBuilder is consumed by .send().

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Plumb `RetryConfig` through `OpenAiCompatibleClient`, `OllamaProvider`, `LmStudioProvider`

This task changes the constructor signatures but does NOT yet wire `send_with_retry` into the call sites. That keeps the diff small and lets us verify the build is green before the behavior change.

**Files:**
- Modify: `crates/ai-providers/src/openai_compat.rs`
- Modify: `crates/ai-providers/src/ollama.rs`
- Modify: `crates/ai-providers/src/lmstudio.rs`

- [ ] **Step 5.1: Add `policy` field to `OpenAiCompatibleClient`**

Edit `crates/ai-providers/src/openai_compat.rs`. Add the import at the top of the file (under existing imports):

```rust
use crate::http_client::RetryConfig;
```

Replace the struct definition + `new` impl (currently at lines 159–171):

```rust
/// A client for any endpoint implementing the OpenAI chat-completions protocol.
pub struct OpenAiCompatibleClient {
    pub client: Client,
    pub base_url: String,
    pub policy: RetryConfig,
}

impl OpenAiCompatibleClient {
    pub fn new(
        client: Client,
        base_url: impl Into<String>,
        policy: RetryConfig,
    ) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            policy,
        }
    }
```

- [ ] **Step 5.2: Update `OllamaProvider::new` to take a policy**

Edit `crates/ai-providers/src/ollama.rs`. Add the import at the top (under existing imports):

```rust
use crate::http_client::RetryConfig;
```

Replace the impl block for `OllamaProvider::new` (lines 19–41):

```rust
impl OllamaProvider {
    /// Create a new Ollama provider.
    ///
    /// `host` defaults to `http://localhost:11434` when `None`.
    /// `policy` controls retry behavior for inner HTTP calls.
    /// Returns `Err(AppError::AiProvider)` if the reqwest client can't be built.
    pub fn new(host: Option<&str>, policy: RetryConfig) -> AppResult<Self> {
        let base = host.unwrap_or("http://localhost:11434");
        let base_url = format!("{base}/v1");
        // No auth header for Ollama.
        let http = Client::builder()
            .pool_max_idle_per_host(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AppError::AiProvider(format!("Failed to build Ollama HTTP client: {e}")))?;
        Ok(Self {
            client: OpenAiCompatibleClient::new(http, base_url, policy),
        })
    }
}
```

Update both existing tests at the bottom of the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_with_default_host() {
        let p = OllamaProvider::new(None, RetryConfig::default()).expect("build default provider");
        assert_eq!(p.client.base_url, "http://localhost:11434/v1");
    }

    #[test]
    fn creates_with_custom_host() {
        let p = OllamaProvider::new(
            Some("http://192.168.1.10:11434"),
            RetryConfig::default(),
        )
        .expect("build custom provider");
        assert_eq!(p.client.base_url, "http://192.168.1.10:11434/v1");
    }
}
```

- [ ] **Step 5.3: Update `LmStudioProvider::new` to take a policy**

Edit `crates/ai-providers/src/lmstudio.rs`. Add the import at the top:

```rust
use crate::http_client::RetryConfig;
```

Replace the impl block (lines 19–42):

```rust
impl LmStudioProvider {
    /// Create a new LM Studio provider.
    ///
    /// `host` defaults to `http://localhost:1234` when `None`.
    /// `policy` controls retry behavior for inner HTTP calls.
    pub fn new(host: Option<&str>, policy: RetryConfig) -> AppResult<Self> {
        let base = host.unwrap_or("http://localhost:1234");
        let base_url = format!("{base}/v1");
        let http = Client::builder()
            .pool_max_idle_per_host(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| AppError::AiProvider(format!("Failed to build LM Studio HTTP client: {e}")))?;
        Ok(Self {
            client: OpenAiCompatibleClient::new(http, base_url, policy),
        })
    }
}
```

Update both existing tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_with_default_host() {
        let p = LmStudioProvider::new(None, RetryConfig::default()).expect("build default provider");
        assert_eq!(p.client.base_url, "http://localhost:1234/v1");
    }

    #[test]
    fn creates_with_custom_host() {
        let p = LmStudioProvider::new(
            Some("http://192.168.1.10:1234"),
            RetryConfig::default(),
        )
        .expect("build custom provider");
        assert_eq!(p.client.base_url, "http://192.168.1.10:1234/v1");
    }
}
```

- [ ] **Step 5.4: Fix the call sites in `state.rs` so the workspace compiles**

Edit `src-tauri/src/state.rs`. Add the import (group with other `medical_ai_providers` imports near the top):

```rust
use medical_ai_providers::http_client::RetryConfig;
```

In `init_ai_providers`, find both `match OllamaProvider::new(...)` and `match LmStudioProvider::new(...)` calls (around lines 114 and 125). Add a single `policy` binding at the top of the function and pass it to both constructors. The complete `init_ai_providers` body becomes:

```rust
pub fn init_ai_providers(config: &AppConfig) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    let policy = RetryConfig::from_app_config(config);

    // Ollama — always available (local, no key needed).
    let ollama_host = if config.ollama_host.is_empty() { "localhost" } else { &config.ollama_host };
    let ollama_url = format!("http://{}:{}", ollama_host, config.ollama_port);
    match OllamaProvider::new(Some(&ollama_url), policy.clone()) {
        Ok(p) => {
            info!(url = %ollama_url, "Registering Ollama provider");
            registry.register(Arc::new(p));
        }
        Err(e) => tracing::error!(error = %e, url = %ollama_url, "Failed to build Ollama provider; skipping"),
    }

    // LM Studio — always available (local or remote, no key needed)
    let lmstudio_host = if config.lmstudio_host.is_empty() { "localhost" } else { &config.lmstudio_host };
    let lmstudio_url = format!("http://{}:{}", lmstudio_host, config.lmstudio_port);
    match LmStudioProvider::new(Some(&lmstudio_url), policy.clone()) {
        Ok(p) => {
            info!(url = %lmstudio_url, "Registering LM Studio provider");
            registry.register(Arc::new(p));
        }
        Err(e) => tracing::error!(error = %e, url = %lmstudio_url, "Failed to build LM Studio provider; skipping"),
    }

    info!("AI providers available: {:?}", registry.list_available());
    registry
}
```

Note: `RetryConfig::clone()` works because `RetryConfig` already derives `Clone` (see existing definition in `http_client.rs`).

- [ ] **Step 5.5: Build and run all tests**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo test -p medical-ai-providers`
Expected: all tests pass (including the 4 updated provider tests).

Run: `cargo test -p rust-medical-assistant-lib`
Expected: all tests pass (state.rs changes don't break anything).

- [ ] **Step 5.6: Commit**

```bash
git add crates/ai-providers/src/openai_compat.rs crates/ai-providers/src/ollama.rs crates/ai-providers/src/lmstudio.rs src-tauri/src/state.rs
git commit -m "$(cat <<'EOF'
refactor(ai-providers): plumb RetryConfig through providers

OpenAiCompatibleClient, OllamaProvider, and LmStudioProvider all now
take a RetryConfig at construction time. init_ai_providers builds it
once via RetryConfig::from_app_config and clones to each provider.

No behavior change yet — call sites still use bare .send().await.
The next tasks wire send_with_retry into each call site.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Wire `send_with_retry` into `OpenAiCompatibleClient::complete()`

This is where behavior changes. The Layer 3 test mounts a wiremock server that returns 503-503-200 for `/v1/chat/completions` and asserts `complete()` recovers.

**Files:**
- Modify: `crates/ai-providers/src/openai_compat.rs`

- [ ] **Step 6.1: Write failing Layer 3 tests**

Edit `crates/ai-providers/src/openai_compat.rs`. Append a `#[cfg(test)] mod tests { ... }` block at the end of the file (the file does not currently have one). Use this complete block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::RetryConfig;
    use medical_core::types::{CompletionRequest, Message, MessageContent, Role};
    use std::time::Duration;

    fn fast_policy(max_retries: u32) -> RetryConfig {
        RetryConfig {
            max_retries,
            initial_delay: Duration::from_millis(20),
            backoff_factor: 2.0,
            max_delay: Duration::from_millis(200),
        }
    }

    fn build_test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("test client")
    }

    fn make_request() -> CompletionRequest {
        CompletionRequest {
            model: "test-model".into(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text("hello".into()),
                tool_calls: vec![],
            }],
            temperature: Some(0.0),
            max_tokens: None,
            system_prompt: None,
        }
    }

    #[tokio::test]
    async fn complete_recovers_from_503() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "test-model",
                "choices": [{
                    "message": {"content": "hi back"},
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;

        let client = OpenAiCompatibleClient::new(
            build_test_client(),
            format!("{}/v1", server.uri()),
            fast_policy(3),
        );

        let resp = client
            .complete(&make_request())
            .await
            .expect("complete should recover");
        assert_eq!(resp.content, "hi back");
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn complete_does_not_retry_400() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let client = OpenAiCompatibleClient::new(
            build_test_client(),
            format!("{}/v1", server.uri()),
            fast_policy(3),
        );

        let err = client
            .complete(&make_request())
            .await
            .expect_err("400 should be permanent");
        let msg = format!("{err}");
        assert!(msg.contains("400"), "expected 400 in error: {msg}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}
```

- [ ] **Step 6.2: Run tests, verify the new tests fail (other tests still pass)**

Run: `cargo test -p medical-ai-providers --lib openai_compat`
Expected: `complete_recovers_from_503` FAILS — wiremock receives 1 request total (the existing non-retrying `complete()` gives up after the first 503). `complete_does_not_retry_400` may pass coincidentally (since there's no retry today). Both fail or behave as described.

- [ ] **Step 6.3: Wire `send_with_retry` into `complete()`**

Edit `crates/ai-providers/src/openai_compat.rs`. Find the `complete` method (around line 351). Replace its body with:

```rust
    pub async fn complete(&self, request: &CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request(request);

        let response = crate::http_client::send_with_retry(&self.policy, || {
            self.client.post(&url).json(&body)
        })
        .await
        .map_err(|e| AppError::AiProvider(e.to_string()))?;

        let status = response.status();
        let raw_body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(AppError::AiProvider(format!("HTTP {status}: {raw_body}")));
        }

        let resp: ChatResponse = serde_json::from_str(&raw_body).map_err(|e| {
            warn!(
                body_preview = &raw_body[..raw_body.len().min(500)],
                "Failed to parse AI response JSON"
            );
            AppError::AiProvider(format!("JSON parse error: {e}"))
        })?;

        debug!(
            url = %url,
            model = %request.model,
            choices = resp.choices.len(),
            "AI completion response received"
        );

        let finish_reason = resp
            .choices
            .first()
            .and_then(|c| c.finish_reason.as_deref())
            .unwrap_or("unknown");
        let has_content = resp
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.as_ref())
            .map(|c| !c.is_empty())
            .unwrap_or(false);

        if !has_content && finish_reason == "length" {
            return Err(AppError::AiProvider(format!(
                "Model '{}' context window exceeded: the prompt is too long for the model, \
                 leaving no room for output. Try a model with a larger context window, \
                 reduce the prompt size, or increase the model's context length in LM Studio.",
                request.model,
            )));
        }

        Ok(self.parse_response(resp, &request.model))
    }
```

- [ ] **Step 6.4: Run tests, verify they pass**

Run: `cargo test -p medical-ai-providers --lib openai_compat`
Expected: both new tests pass; existing tests still pass.

- [ ] **Step 6.5: Commit**

```bash
git add crates/ai-providers/src/openai_compat.rs
git commit -m "$(cat <<'EOF'
feat(ai-providers): wire send_with_retry into complete()

Transient 503/429/etc responses from local providers now retry
automatically with exponential backoff + jitter. Layer 3 tests verify
recovery from a 503-503-200 sequence and that 400s still fail fast.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Wire `send_with_retry` into `complete_stream()` with Layer 3 test

**Files:**
- Modify: `crates/ai-providers/src/openai_compat.rs`

- [ ] **Step 7.1: Write failing test**

Edit `crates/ai-providers/src/openai_compat.rs`. Inside the existing `mod tests` block (added in Task 6), append:

```rust
    #[tokio::test]
    async fn complete_stream_retries_initial_send() {
        use futures_util::StreamExt;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;

        // First two POSTs to /v1/chat/completions return 503 — the initial
        // SSE send should retry them.
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;

        // Third POST returns a minimal SSE stream with one delta and a usage chunk.
        let sse_body = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
                        data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n\
                        data: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let client = OpenAiCompatibleClient::new(
            build_test_client(),
            format!("{}/v1", server.uri()),
            fast_policy(3),
        );

        let mut stream = client
            .complete_stream(&make_request())
            .await
            .expect("stream should be established after retries");

        // Drain the stream; ensure at least one Delta with text "hi" is observed.
        let mut saw_delta = false;
        while let Some(item) = stream.next().await {
            let chunk = item.expect("no stream errors");
            if let medical_core::types::StreamChunk::Delta { text } = chunk {
                if text == "hi" {
                    saw_delta = true;
                }
            }
        }
        assert!(saw_delta, "expected to see 'hi' delta");
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }
```

- [ ] **Step 7.2: Run test, verify it fails**

Run: `cargo test -p medical-ai-providers --lib complete_stream_retries_initial_send`
Expected: FAIL — current `complete_stream` does not retry, so it sees the first 503 and bubbles up an error.

- [ ] **Step 7.3: Wire `send_with_retry` into `complete_stream()`**

Edit `crates/ai-providers/src/openai_compat.rs`. Find the `complete_stream` method (around line 404). Replace the initial `send().await` block. Specifically, locate this:

```rust
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;
```

…and replace with:

```rust
        let response = crate::http_client::send_with_retry(&self.policy, || {
            self.client.post(&url).json(&body)
        })
        .await
        .map_err(|e| AppError::AiProvider(e.to_string()))?;
```

The rest of `complete_stream` (status check, SSE parsing, chunk mapping) is unchanged — mid-stream errors are intentionally NOT retried (per spec § Streaming behavior).

- [ ] **Step 7.4: Run tests, verify they pass**

Run: `cargo test -p medical-ai-providers --lib openai_compat`
Expected: all openai_compat tests pass.

- [ ] **Step 7.5: Commit**

```bash
git add crates/ai-providers/src/openai_compat.rs
git commit -m "$(cat <<'EOF'
feat(ai-providers): retry initial send for streaming completions

complete_stream now retries the initial POST exactly the same way
as the non-streaming path. Once the SSE response is established and
chunks are flowing, mid-stream errors still bubble up unchanged
(by design — re-prompting would reset the user's partial output).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Wire `send_with_retry` into `complete_with_tools()` and `list_models()`

These two call sites get the same treatment as `complete()` but without bespoke Layer 3 tests — coverage from Task 6's `complete_recovers_from_503` plus the Layer 2 tests already exercise the retry path through identical code.

**Files:**
- Modify: `crates/ai-providers/src/openai_compat.rs`

- [ ] **Step 8.1: Wire `complete_with_tools`**

Edit `crates/ai-providers/src/openai_compat.rs`. Find the `complete_with_tools` method (around line 489). Locate this block:

```rust
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;
```

…and replace with:

```rust
        let response = crate::http_client::send_with_retry(&self.policy, || {
            self.client.post(&url).json(&body)
        })
        .await
        .map_err(|e| AppError::AiProvider(e.to_string()))?;
```

- [ ] **Step 8.2: Wire `list_models`**

Edit `crates/ai-providers/src/openai_compat.rs`. Find `list_models` (around line 326). Locate:

```rust
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::AiProvider(e.to_string()))?;
```

…and replace with:

```rust
        let response = crate::http_client::send_with_retry(&self.policy, || {
            self.client.get(&url)
        })
        .await
        .map_err(|e| AppError::AiProvider(e.to_string()))?;
```

- [ ] **Step 8.3: Run tests**

Run: `cargo test -p medical-ai-providers`
Expected: all tests pass.

- [ ] **Step 8.4: Commit**

```bash
git add crates/ai-providers/src/openai_compat.rs
git commit -m "$(cat <<'EOF'
feat(ai-providers): retry list_models and complete_with_tools

The remaining two HTTP call sites in OpenAiCompatibleClient now go
through send_with_retry. Coverage rides on the existing complete()
Layer 3 tests since the retry path is identical.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: End-to-end verification — workspace tests, clippy, manual smoke

This task does not modify code. It runs the full quality bar to confirm the change is shippable and includes a manual smoke step against a running app.

- [ ] **Step 9.1: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass. Note any failures; investigate before declaring done.

- [ ] **Step 9.2: Run clippy for the ai-providers crate at strict level**

Run: `cargo clippy -p medical-ai-providers --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 9.3: Run clippy for the entire workspace**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no new warnings introduced by this branch. (Pre-existing warnings — `unused variable: 'after'` in `crates/security/src/machine_id.rs` and `value assigned to 'offset' is never read` in `crates/stt-providers/src/diarization.rs` — were present before this work and are out of scope.)

- [ ] **Step 9.4: Verify the spec acceptance criteria by code inspection**

Open `docs/superpowers/specs/2026-04-29-ai-provider-retry-design.md` and confirm:

1. **Spec criterion 1** — "503 → second/third attempt succeeds": Layer 2 `send_with_retry_retries_on_503_then_succeeds` and Layer 3 `complete_recovers_from_503` cover this.
2. **Spec criterion 2** — "400 surfaces immediately": Layer 2 `send_with_retry_does_not_retry_400` and Layer 3 `complete_does_not_retry_400` cover this.
3. **Spec criterion 3** — "auto_retry_failed=false → no retries": Layer 2 `send_with_retry_does_not_retry_when_disabled` covers this; Layer 1 `from_app_config_disabled_zero_retries` covers the settings wiring.
4. **Spec criterion 4** — "Layers 1+2+3 pass in CI": confirmed by step 9.1.
5. **Spec criterion 5** — "no new clippy warnings; new dep `rand` already universally common": confirmed by step 9.3.

- [ ] **Step 9.5: Manual smoke test against the running app**

This is a one-time human verification — Ollama (or LM Studio) must be running locally. The point is to confirm a real provider sees the retry path without a 503 in the mix.

1. Ensure no orphaned dev server is on port 1420: `lsof -nP -iTCP:1420 -sTCP:LISTEN`. Kill any leftover `npm run dev` / `vite` PID before continuing.
2. Run: `npm run tauri dev`
3. Once the app is up, perform a SOAP generation against the active provider. Watch logs for either:
   - No retry messages and a successful generation (the happy path — confirms no regression).
   - `AI provider transient failure, retrying` followed by `AI provider recovered after retries` (confirms the retry path is wired).
4. With the provider running, intentionally trigger a 4xx by setting `ai_model` in settings to a model name the provider doesn't have. Confirm the error surfaces immediately (no retry storm — single error, fast fail).

If steps 1–4 behave as described, the change is ready.

- [ ] **Step 9.6: If everything passes, no commit is needed for this task.** If a fix is required, commit it as a follow-up `fix(ai-providers): ...` with the reason in the body.

---

## Self-review notes

This plan was self-reviewed against `docs/superpowers/specs/2026-04-29-ai-provider-retry-design.md` after writing:

- **Spec coverage:** every requirement in the spec maps to at least one task. Spec § Components → Tasks 1–8. Spec § Settings integration → Tasks 1, 5. Spec § Control flow → Task 4. Spec § Error classification → Task 3. Spec § Streaming behavior → Task 7. Spec § Logging → Task 4 implementation. Spec § Testing strategy Layer 1 → Tasks 1–3. Layer 2 → Task 4. Layer 3 → Tasks 6–7. Spec § Acceptance criteria → Task 9.
- **Placeholder scan:** no TBD/TODO/"implement later" placeholders. Every step has either exact code or an exact command.
- **Type consistency:** `RetryConfig`, `RetryDecision`, `parse_retry_after`, `classify`, `classify_status`, `classify_error`, `send_with_retry`, `OpenAiCompatibleClient`, `OllamaProvider`, `LmStudioProvider`, `AppConfig` — names match across all task references.
- **Scope deviation flagged:** HTTP-date support for `Retry-After` is dropped (see top of plan, "Scope reduction"). Spec marked it best-effort; this plan returns `None` instead, which gracefully falls back to the configured backoff.
