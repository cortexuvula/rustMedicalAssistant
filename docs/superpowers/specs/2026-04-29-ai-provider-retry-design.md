# AI Provider Retry — Design

**Status:** approved 2026-04-29
**Author:** brainstorm session, 2026-04-29
**Scope:** v1 — retry only. No failover, no circuit breaker, no UI changes.

## Problem

AI completion calls (`OpenAiCompatClient::complete`, `complete_stream`, `complete_with_tools`, `list_models`) bubble any non-2xx response straight to the user as `AppError::AiProvider(...)`. A transient 503 from a busy local LLM server (Ollama or LM Studio) — observed in production on 2026-04-29 with `{"error":"Server overloaded, please retry shortly (ref: ...)"}` — fails the SOAP generation outright. The user has to retry by hand.

`crates/ai-providers/src/http_client.rs` already defines `RetryConfig` and `CircuitBreaker` types with passing unit tests, but **nothing in the codebase uses them**. `crates/core/src/types/settings.rs` similarly defines `auto_retry_failed: bool` and `max_retry_attempts: u32` fields on `AppConfig`, also unused.

This design wires the retry path through, leaving the circuit breaker and any failover work for a future iteration.

## Non-goals

- **Failover** between providers. We use only local Ollama and LM Studio (HIPAA constraint); the user picks one in Settings and we don't silently swap during a clinical workflow.
- **Circuit breaker.** Marginal value for a single-user local app where connection-refused already fails fast.
- **Mid-stream SSE retry.** Once an SSE stream is established and the client has received some chunks, retrying would re-prompt the LLM and the user would see content reset. Out of scope.
- **Frontend Settings UI changes.** The two existing settings fields are read from `~/Library/Application Support/rust-medical-assistant/config.json`; users can edit it directly if they need to disable retries. UI work is a future iteration.
- **New settings fields.** Tuning constants (initial delay, backoff factor, max cap, jitter ratio) stay hardcoded.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  openai_compat.rs                                            │
│    list_models()                                             │
│    complete()           ─┐                                   │
│    complete_stream()    ─┼──► send_with_retry(..)  ◄─┐       │
│    complete_with_tools()─┘    (new, in http_client.rs)│      │
│                                       │              │       │
│                                       │ uses         │       │
│                                       ▼              │       │
│                              RetryConfig (existing)──┘       │
│                                + classify()                  │
│                                + parse_retry_after()         │
│                                + jittered()                  │
└──────────────────────────────────────────────────────────────┘

  AppConfig.auto_retry_failed   ─┐
  AppConfig.max_retry_attempts  ─┴──► RetryConfig::from_app_config(&cfg)
                                       (constructed at provider build time)
```

`send_with_retry` is the single, shared implementation; per-provider call sites just wrap their existing `.send()` with it. The function takes a closure (`Fn() -> RequestBuilder`) because `reqwest::RequestBuilder` is consumed by `.send()` and must be rebuilt on each attempt.

## Components

### New code in `crates/ai-providers/src/http_client.rs`

```rust
impl RetryConfig {
    /// Construct policy from user-facing settings.
    /// `auto_retry_failed=false` → max_retries=0 (no retries).
    /// `max_retry_attempts` propagates as-is.
    /// Other fields stay at Default::default(): initial_delay=1s, backoff_factor=2.0, max_delay=30s.
    pub fn from_app_config(cfg: &AppConfig) -> Self;

    /// Apply ±25% jitter to a base delay. Pure function for testability.
    /// Caller passes its own RNG so tests can use a seeded one.
    pub fn jittered(&self, base: Duration, rng: &mut impl Rng) -> Duration;
}

/// Classification of a request outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum RetryDecision {
    Success,
    Permanent,
    Transient,
    TransientWithDelay(Duration),
}

/// Inspect a reqwest result and classify whether retry is appropriate.
pub fn classify(result: &Result<reqwest::Response, reqwest::Error>) -> RetryDecision;

/// Parse a `Retry-After` HTTP header value (delta-seconds or HTTP-date).
/// Returns None if absent or malformed. HTTP-date support is best-effort.
pub fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration>;

/// Send a request with retry/backoff per the configured policy.
/// `factory` is called fresh on each attempt because RequestBuilder is consumed by send().
pub async fn send_with_retry<F>(
    policy: &RetryConfig,
    factory: F,
) -> Result<reqwest::Response, reqwest::Error>
where
    F: Fn() -> reqwest::RequestBuilder;
```

### Modified files

| File | Change |
|------|--------|
| `crates/ai-providers/src/http_client.rs` | Add 4 functions above + tests; add `rand` for jitter; ~120 LOC. |
| `crates/ai-providers/src/openai_compat.rs` | Store `policy: RetryConfig` on `OpenAiCompatClient`. Replace 4 `.send().await` sites with `send_with_retry(&self.policy, \|\| ...)`. ~20 LOC net. |
| `crates/ai-providers/src/ollama.rs` | If it constructs `OpenAiCompatClient`, pass policy through (verify in plan). |
| `crates/ai-providers/src/lmstudio.rs` | Same as ollama.rs. |
| `crates/ai-providers/Cargo.toml` | Add `rand = "0.8"` if not already a transitive dep; add `wiremock = "0.6"` as dev-dep. |
| `src-tauri/src/state.rs` | Pass `&AppConfig` to provider constructors so each provider gets a `RetryConfig::from_app_config(&cfg)` at registration time. |
| `crates/core/src/types/settings.rs` | **No change** — `auto_retry_failed` and `max_retry_attempts` already present. |

### Settings integration

```rust
impl RetryConfig {
    pub fn from_app_config(cfg: &AppConfig) -> Self {
        let max_retries = if cfg.auto_retry_failed {
            cfg.max_retry_attempts
        } else {
            0
        };
        Self {
            max_retries,
            initial_delay: Duration::from_secs(1),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(30),
        }
    }
}
```

`max_retry_attempts` is the count of *retries* after the initial attempt — total request count = 1 + max_retries. With the default `max_retry_attempts=3`, the wait sequence is 1s → 2s → 4s (before jitter); total wall-clock budget ~7s plus the actual HTTP work.

The settings are read **once** at provider construction time in `state.rs`. Changing the settings at runtime would not retroactively update existing `OpenAiCompatClient` instances; that's acceptable for v1 since provider switches already trigger re-registration.

## Control flow

```
attempt = 0
loop:
    response_or_error = factory().send().await
    decision = classify(&response_or_error)

    match decision:
        Success:
            if attempt > 0:
                info!("AI provider recovered after {attempt} retries")
            return Ok(response)
        Permanent:
            return response_or_error                       # bubble up unchanged
        Transient:
            if attempt >= policy.max_retries:
                return response_or_error                   # give up; surface last failure
            delay = policy.jittered(policy.delay_for_attempt(attempt), &mut thread_rng)
        TransientWithDelay(server_delay):
            if attempt >= policy.max_retries:
                return response_or_error
            delay = min(server_delay, policy.max_delay)    # honor Retry-After, capped at 30s

    info!(
        attempt = attempt + 1,
        max = policy.max_retries + 1,
        delay_ms = delay.as_millis(),
        host = ...,
        status = ...,
        "AI provider transient failure, retrying",
    )
    tokio::time::sleep(delay).await
    attempt += 1
```

## Error classification

`classify(result) -> RetryDecision`:

| Input | Decision | Rationale |
|-------|----------|-----------|
| `Ok(resp)` with `resp.status().is_success()` | `Success` | 2xx |
| `Ok(resp)` with status ∈ {408, 429, 500, 502, 503, 504} | `TransientWithDelay(d)` if `Retry-After` parses to `Some(d)`, else `Transient` | Documented transient codes |
| `Ok(resp)` with any other 4xx or 5xx (400, 401, 403, 404, 405, 409, 410, 413, 414, 415, 422, 501, 505) | `Permanent` | Bad input, bad model name, unsupported method — retrying won't change the outcome |
| `Err(e)` where `e.is_connect()` | `Permanent` | Connection refused → "is Ollama running?" Fail fast (no point waiting 7s for the same answer) |
| `Err(e)` where `e.is_timeout()` | `Transient` | Read or connect timeout |
| `Err(e)` where `e.is_request()` (other transport) | `Transient` | Conservative — covers dropped connection, TLS hiccup |
| `Err(e)` where `e.is_body()` or `e.is_decode()` | `Permanent` | Server returned malformed data |
| Any other `Err(e)` | `Permanent` | Default safe |

`reqwest::Error::is_*` methods overlap; check order is `is_connect → is_timeout → is_request → is_body → is_decode → fallthrough`.

## Streaming behavior

`complete_stream` retries **only the initial `send()`**; once a 2xx response is established and the SSE parser is consuming chunks, errors bubble to the UI as today.

Implementation-wise both call sites (`complete` and `complete_stream`) call `send_with_retry` identically. The difference is what they do with the returned `Response` — `complete` parses JSON, `complete_stream` hands it to `parse_sse_response`. There is no special-casing inside `send_with_retry`.

## Logging

- Per-retry: `tracing::info!` with `attempt`, `max`, `delay_ms`, `status`, `host`. Logged just before the sleep.
- Successful recovery: `tracing::info!("AI provider recovered after {n} retries")` exactly once, after the successful response is received.
- Final failure: existing `AppError::AiProvider(...)` path is unchanged. The error message includes the last status and body preview.

No PHI is logged at any point — only metadata (status code, host, attempt counts).

## Testing strategy

Three layers, all in `crates/ai-providers/`.

### Layer 1 — Pure unit tests in `http_client.rs`

| Test | Verifies |
|------|----------|
| `from_app_config_disabled_zero_retries` | `auto_retry_failed=false` produces `max_retries=0` |
| `from_app_config_reads_max_attempts` | `max_retry_attempts=5` propagates |
| `jittered_within_25_percent_band` | jittered output ∈ [0.75 × base, 1.25 × base] over many samples |
| `parse_retry_after_seconds` | `"30"` → `Some(30s)` |
| `parse_retry_after_http_date` | RFC 1123 date parses to a positive Duration |
| `parse_retry_after_malformed_returns_none` | `"banana"` → `None` |
| `parse_retry_after_missing_returns_none` | header absent → `None` |
| `classify_2xx_success` | 200, 201, 204 → `Success` |
| `classify_503_transient` | 503 without Retry-After → `Transient` |
| `classify_503_with_retry_after` | 503 with `Retry-After: 5` → `TransientWithDelay(5s)` |
| `classify_429_with_retry_after` | 429 with `Retry-After: 2` → `TransientWithDelay(2s)` |
| `classify_400_permanent` | 400 → `Permanent` |
| `classify_401_permanent` | 401 → `Permanent` |
| `classify_404_permanent` | 404 → `Permanent` |
| `classify_422_permanent` | 422 → `Permanent` |
| `classify_connect_refused_permanent` | connection-refused error → `Permanent` |
| `classify_timeout_transient` | timeout error → `Transient` |

### Layer 2 — `send_with_retry` integration tests (new file, uses `wiremock`)

| Test | Mock setup | Asserts |
|------|------------|---------|
| `succeeds_first_try` | server always returns 200 | exactly 1 request, returns Ok |
| `retries_on_503_then_succeeds` | 503, 503, 200 in order | exactly 3 requests, returns Ok, total wait time ≥ ~2.25s (1s + 2s before −25% jitter) |
| `gives_up_after_max_attempts` | server always 503 | exactly 4 requests (initial + 3 retries), returns Err with status 503 in body |
| `does_not_retry_400` | server returns 400 | exactly 1 request |
| `does_not_retry_401` | server returns 401 | exactly 1 request |
| `does_not_retry_when_disabled` | server returns 503; policy with `max_retries=0` | exactly 1 request |
| `honors_retry_after_header` | 503 with `Retry-After: 1` then 200 | gap ≥ ~1s (within jitter band) |
| `caps_retry_after_at_max_delay` | 503 with `Retry-After: 9999` then 200 | gap ≤ 30s |

### Layer 3 — Provider-level integration test (new, uses `wiremock`)

| Test | Asserts |
|------|---------|
| `openai_compat_complete_recovers_from_503` | `OpenAiCompatClient::complete()` succeeds when wiremock returns 503, 503, 200; `CompletionResponse` correctly parsed from final 200 |
| `openai_compat_complete_stream_retries_initial_send` | streaming initial 503 retries; once 200 response is established, SSE chunks pass through unchanged |
| `openai_compat_complete_does_not_retry_400` | a 400 surfaces immediately as `AppError::AiProvider` after exactly 1 request |

### Out of test scope

- Mid-stream SSE retry (out of v1 scope per Streaming section).
- Real Ollama / LM Studio (would require running them in CI; existing tests don't do that either).
- Frontend behavior (no UI changes in v1).
- Circuit breaker behavior (skipped per Non-goals).

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Retry budget hides a real provider problem from the user | Per-retry `info!` logs and final-failure error message preserve full detail; daily user is unlikely to be confused by a 7s pause they'd otherwise have spent retrying manually |
| `RequestBuilder` rebuild on each attempt re-clones the body (large prompts allocate twice on each retry) | Acceptable — payload is bytes, not seconds; for typical SOAP transcripts <1MB this is microseconds |
| Settings read once at construction means a runtime change to `auto_retry_failed` doesn't take effect until provider re-registration | Acceptable — provider switches already trigger re-registration in `state.rs`; this is consistent with how other settings are handled |
| `wiremock` adds CI test time | Wiremock is already a dev-dep in `stt-providers`; tests are in-process and fast (~tens of ms each) |
| Jitter using `thread_rng()` makes some tests flaky | Tests that assert on timing use bands (≥0.75× / ≤1.25×) rather than exact values; pure unit tests pass a seeded RNG |

## Acceptance criteria

1. After this change, today's incident — a 503 with `{"error":"Server overloaded, please retry shortly"}` from a local provider — succeeds automatically on the second or third attempt, without user intervention, and the SOAP note is generated.
2. A `400 Bad Request` (e.g. wrong model name) still surfaces immediately to the user with the same error text it does today.
3. With `auto_retry_failed=false` in config.json, the app behaves exactly as today (no retries).
4. All Layer 1 + Layer 2 + Layer 3 tests pass in CI.
5. No new clippy warnings; no new dependency that isn't either already present (transitively or otherwise) or universally common (`rand`).
