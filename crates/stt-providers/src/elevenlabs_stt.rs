//! ElevenLabs Scribe v2 speech-to-text provider.

use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use reqwest::{Client, header, multipart};
use serde::Deserialize;
use tracing::instrument;

use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::{AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk};

use crate::deepgram::encode_audio_to_wav;

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ElevenLabsResponse {
    text: Option<String>,
}

// ── Provider ─────────────────────────────────────────────────────────────────

/// ElevenLabs Scribe v2 speech-to-text provider.
pub struct ElevenLabsSttProvider {
    client: Client,
}

impl ElevenLabsSttProvider {
    /// Create a new provider using the given ElevenLabs API key.
    ///
    /// Uses `xi-api-key` authentication and a 300-second timeout.
    pub fn new(api_key: &str) -> AppResult<Self> {
        let mut key_value =
            header::HeaderValue::from_str(api_key)
                .map_err(|e| AppError::SttProvider(format!("Invalid API key header: {e}")))?;
        key_value.set_sensitive(true);

        let header_name = header::HeaderName::from_static("xi-api-key");

        let mut headers = header::HeaderMap::new();
        headers.insert(header_name, key_value);

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
impl SttProvider for ElevenLabsSttProvider {
    fn name(&self) -> &str {
        "elevenlabs"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_diarization(&self) -> bool {
        true
    }

    #[instrument(skip(self, audio, config), fields(provider = "elevenlabs"))]
    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let duration = audio.duration_seconds();
        let wav_bytes = encode_audio_to_wav(&audio)?;

        let file_part = multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| AppError::SttProvider(format!("MIME type: {e}")))?;

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model_id", "scribe_v2");

        if let Some(ref lang) = config.language {
            form = form.text("language_code", lang.clone());
        }

        if config.diarize {
            form = form.text("diarize", "true");
            if let Some(n) = config.num_speakers {
                form = form.text("num_speakers", n.to_string());
            }
        }

        let response = self
            .client
            .post("https://api.elevenlabs.io/v1/speech-to-text")
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::SttProvider(format!("ElevenLabs request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::SttProvider(format!(
                "ElevenLabs HTTP {status}: {body}"
            )));
        }

        let el: ElevenLabsResponse = response
            .json()
            .await
            .map_err(|e| AppError::SttProvider(format!("ElevenLabs JSON: {e}")))?;

        let text = el.text.unwrap_or_default();

        Ok(Transcript {
            text,
            segments: vec![],
            language: config.language.clone(),
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
            "elevenlabs: streaming not yet implemented".to_owned(),
        ))
    }
}
