//! Groq Whisper Large-v3-Turbo STT provider.

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
struct GroqResponse {
    text: Option<String>,
}

// ── Provider ─────────────────────────────────────────────────────────────────

/// Groq Whisper Large-v3-Turbo speech-to-text provider.
pub struct GroqWhisperProvider {
    client: Client,
}

impl GroqWhisperProvider {
    /// Create a new provider using the given Groq API key.
    ///
    /// Uses Bearer authentication and a 120-second timeout.
    pub fn new(api_key: &str) -> AppResult<Self> {
        let mut auth_value =
            header::HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|e| AppError::SttProvider(format!("Invalid API key header: {e}")))?;
        auth_value.set_sensitive(true);

        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, auth_value);

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::SttProvider(format!("HTTP client build: {e}")))?;

        Ok(Self { client })
    }
}

#[async_trait]
impl SttProvider for GroqWhisperProvider {
    fn name(&self) -> &str {
        "groq_whisper"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_diarization(&self) -> bool {
        false
    }

    #[instrument(skip(self, audio, config), fields(provider = "groq_whisper"))]
    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let duration = audio.duration_seconds();
        let wav_bytes = encode_audio_to_wav(&audio)?;

        // Build multipart form.
        let file_part = multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| AppError::SttProvider(format!("MIME type: {e}")))?;

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", "whisper-large-v3-turbo");

        if let Some(ref lang) = config.language {
            // Groq expects ISO-639-1 (2-char) language codes.
            let lang_code = lang.chars().take(2).collect::<String>();
            form = form.text("language", lang_code);
        }

        let response = self
            .client
            .post("https://api.groq.com/openai/v1/audio/transcriptions")
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::SttProvider(format!("Groq request: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::SttProvider(format!(
                "Groq HTTP {status}: {body}"
            )));
        }

        let groq: GroqResponse = response
            .json()
            .await
            .map_err(|e| AppError::SttProvider(format!("Groq JSON: {e}")))?;

        let text = groq.text.unwrap_or_default();

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
            "groq_whisper: streaming not yet implemented".to_owned(),
        ))
    }
}
