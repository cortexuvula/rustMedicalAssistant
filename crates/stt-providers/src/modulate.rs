//! Modulate/Velma STT provider.
//!
//! Features: emotion detection, diarization, deepfake detection, PII redaction.
//!
//! This provider sends audio as WAV via HTTP POST to the Modulate transcription
//! API endpoint and parses a JSON transcript response.

use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use reqwest::{Client, header};
use serde::Deserialize;
use tracing::instrument;

use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::{AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk, TranscriptSegment};

use crate::deepgram::encode_audio_to_wav;

/// Base URL for the Modulate transcription API.
///
/// This is a placeholder — replace with the actual endpoint once credentials
/// and documentation are available.
const MODULATE_API_URL: &str = "https://api.modulate.ai/v1/transcribe";

// ── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ModulateResponse {
    /// The full transcript text.
    text: Option<String>,
    /// Word-level segments with timing and optional speaker labels.
    segments: Option<Vec<ModulateSegment>>,
    /// Detected language code (BCP-47).
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModulateSegment {
    text: Option<String>,
    start: Option<f64>,
    end: Option<f64>,
    speaker: Option<String>,
    confidence: Option<f32>,
}

// ── Provider ────────────────────────────────────────────────────────────────

/// Modulate/Velma speech-to-text provider.
pub struct ModulateProvider {
    client: Client,
}

impl ModulateProvider {
    /// Create a new provider with the given Modulate API key.
    ///
    /// Uses `Authorization: Bearer {key}` authentication and a 300-second
    /// timeout to accommodate long audio files.
    pub fn new(api_key: &str) -> AppResult<Self> {
        let mut auth_value = header::HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|e| AppError::SttProvider(format!("Invalid API key header: {e}")))?;
        auth_value.set_sensitive(true);

        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, auth_value);

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::SttProvider(format!("HTTP client build: {e}")))?;

        Ok(Self { client })
    }
}

#[async_trait]
impl SttProvider for ModulateProvider {
    fn name(&self) -> &str {
        "modulate"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_diarization(&self) -> bool {
        true
    }

    #[instrument(skip(self, audio, config), fields(provider = "modulate"))]
    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let duration = audio.duration_seconds();
        let wav_bytes = encode_audio_to_wav(&audio)?;

        // Build query parameters.
        let mut url = MODULATE_API_URL.to_owned();
        let mut first_param = true;

        let mut append_param = |key: &str, value: &str| {
            if first_param {
                url.push('?');
                first_param = false;
            } else {
                url.push('&');
            }
            url.push_str(key);
            url.push('=');
            url.push_str(value);
        };

        if config.diarize {
            append_param("diarize", "true");
            if let Some(n) = config.num_speakers {
                append_param("num_speakers", &n.to_string());
            }
        }

        if let Some(ref lang) = config.language {
            append_param("language", lang);
        }

        let response = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "audio/wav")
            .body(wav_bytes)
            .send()
            .await
            .map_err(|e| AppError::SttProvider(format!("Modulate request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::SttProvider(format!(
                "Modulate HTTP {status}: {body}"
            )));
        }

        let mr: ModulateResponse = response
            .json()
            .await
            .map_err(|e| AppError::SttProvider(format!("Modulate JSON: {e}")))?;

        let text = mr.text.unwrap_or_default();

        let segments: Vec<TranscriptSegment> = mr
            .segments
            .map(|segs| {
                segs.into_iter()
                    .filter_map(|s| {
                        Some(TranscriptSegment {
                            text: s.text?,
                            start: s.start?,
                            end: s.end?,
                            speaker: s.speaker,
                            confidence: s.confidence,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Transcript {
            text,
            segments,
            language: mr.language.or(config.language.clone()),
            duration_seconds: Some(duration),
            provider: self.name().to_owned(),
            metadata: serde_json::Value::Null,
        })
    }

    async fn transcribe_stream(
        &self,
        _stream: AudioStream,
        _config: SttConfig,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send + Unpin>> {
        Err(AppError::SttProvider(
            "modulate: streaming transcription is not supported".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name() {
        // Use a dummy key just to test construction.
        let provider = ModulateProvider::new("test-key-12345").expect("construction should succeed");
        assert_eq!(provider.name(), "modulate");
    }

    #[test]
    fn supports_diarization_true() {
        let provider = ModulateProvider::new("test-key").unwrap();
        assert!(provider.supports_diarization());
    }

    #[test]
    fn supports_streaming_false() {
        let provider = ModulateProvider::new("test-key").unwrap();
        assert!(!provider.supports_streaming());
    }

    #[test]
    fn invalid_api_key_rejected() {
        // Header values cannot contain certain characters.
        let result = ModulateProvider::new("key\nwith\nnewlines");
        assert!(result.is_err());
    }

    #[test]
    fn modulate_response_deserializes() {
        let json = r#"{
            "text": "The patient reports chest pain.",
            "segments": [
                {
                    "text": "The patient",
                    "start": 0.0,
                    "end": 0.5,
                    "speaker": "speaker_0",
                    "confidence": 0.95
                },
                {
                    "text": "reports chest pain.",
                    "start": 0.5,
                    "end": 1.2,
                    "speaker": "speaker_0",
                    "confidence": 0.92
                }
            ],
            "language": "en"
        }"#;

        let resp: ModulateResponse = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(resp.text.as_deref(), Some("The patient reports chest pain."));
        assert_eq!(resp.language.as_deref(), Some("en"));

        let segs = resp.segments.unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].speaker.as_deref(), Some("speaker_0"));
    }

    #[test]
    fn modulate_response_deserializes_minimal() {
        let json = r#"{"text": "hello"}"#;
        let resp: ModulateResponse = serde_json::from_str(json).expect("should deserialize");
        assert_eq!(resp.text.as_deref(), Some("hello"));
        assert!(resp.segments.is_none());
        assert!(resp.language.is_none());
    }
}
