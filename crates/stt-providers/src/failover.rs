//! Failover chain with per-provider circuit-breaker health tracking.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use medical_core::error::AppResult;
use medical_core::traits::SttProvider;
use medical_core::types::{AudioData, SttConfig, Transcript};

/// Tracks health state for a single provider.
#[derive(Debug)]
pub struct ProviderHealth {
    pub failure_count: u32,
    pub last_failure: Option<Instant>,
}

impl ProviderHealth {
    pub fn new() -> Self {
        Self {
            failure_count: 0,
            last_failure: None,
        }
    }

    /// Returns `true` when the provider is available (circuit not open).
    pub fn is_available(&self, threshold: u32, cooldown: Duration) -> bool {
        if self.failure_count < threshold {
            return true;
        }
        // Threshold reached — check if cooldown has elapsed (half-open / recovery).
        match self.last_failure {
            None => true,
            Some(t) => t.elapsed() >= cooldown,
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

impl Default for ProviderHealth {
    fn default() -> Self {
        Self::new()
    }
}

/// Ordered failover chain of STT providers with circuit-breaker health tracking.
pub struct SttFailover {
    chain: Vec<Arc<dyn SttProvider>>,
    health: Mutex<HashMap<String, ProviderHealth>>,
    failure_threshold: u32,
    cooldown: Duration,
}

impl SttFailover {
    /// Create a new failover chain with default thresholds (3 failures, 300 s cooldown).
    pub fn new(chain: Vec<Arc<dyn SttProvider>>) -> Self {
        let mut map = HashMap::new();
        for p in &chain {
            map.insert(p.name().to_owned(), ProviderHealth::new());
        }
        Self {
            chain,
            health: Mutex::new(map),
            failure_threshold: 3,
            cooldown: Duration::from_secs(300),
        }
    }

    /// Builder-style override for thresholds.
    pub fn with_thresholds(mut self, failure_threshold: u32, cooldown_secs: u64) -> Self {
        self.failure_threshold = failure_threshold;
        self.cooldown = Duration::from_secs(cooldown_secs);
        self
    }

    /// Try each provider in order, skipping those whose circuit is open.
    /// Returns the first successful transcript, or the last encountered error.
    pub async fn transcribe(
        &self,
        audio: AudioData,
        config: SttConfig,
    ) -> AppResult<Transcript> {
        let mut last_err = medical_core::error::AppError::SttProvider(
            "All providers exhausted".to_owned(),
        );

        for provider in &self.chain {
            let name = provider.name().to_owned();

            // Check circuit-breaker state.
            let available = {
                let guard = self.health.lock().unwrap();
                guard
                    .get(&name)
                    .map(|h| h.is_available(self.failure_threshold, self.cooldown))
                    .unwrap_or(true)
            };

            if !available {
                tracing::debug!(provider = %name, "skipping — circuit open");
                continue;
            }

            match provider.transcribe(audio.clone(), config.clone()).await {
                Ok(transcript) => {
                    let mut guard = self.health.lock().unwrap();
                    if let Some(h) = guard.get_mut(&name) {
                        h.record_success();
                    }
                    return Ok(transcript);
                }
                Err(e) => {
                    tracing::warn!(provider = %name, error = %e, "transcription failed");
                    let mut guard = self.health.lock().unwrap();
                    if let Some(h) = guard.get_mut(&name) {
                        h.record_failure();
                    }
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }

    /// Returns a snapshot of `(provider_name, is_available)` for each provider in the chain.
    pub fn provider_statuses(&self) -> Vec<(String, bool)> {
        let guard = self.health.lock().unwrap();
        self.chain
            .iter()
            .map(|p| {
                let name = p.name().to_owned();
                let available = guard
                    .get(&name)
                    .map(|h| h.is_available(self.failure_threshold, self.cooldown))
                    .unwrap_or(true);
                (name, available)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_starts_available() {
        let h = ProviderHealth::new();
        assert!(h.is_available(3, Duration::from_secs(300)));
    }

    #[test]
    fn unavailable_after_threshold() {
        let mut h = ProviderHealth::new();
        h.record_failure();
        h.record_failure();
        h.record_failure();
        // Three failures — circuit should be open.
        assert!(!h.is_available(3, Duration::from_secs(300)));
    }

    #[test]
    fn resets_on_success() {
        let mut h = ProviderHealth::new();
        h.record_failure();
        h.record_failure();
        h.record_failure();
        assert!(!h.is_available(3, Duration::from_secs(300)));
        h.record_success();
        assert!(h.is_available(3, Duration::from_secs(300)));
    }

    #[test]
    fn creates_with_empty_chain() {
        let failover = SttFailover::new(vec![]);
        let statuses = failover.provider_statuses();
        assert!(statuses.is_empty());
    }
}
