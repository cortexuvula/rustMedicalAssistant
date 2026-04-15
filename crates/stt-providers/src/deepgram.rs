//! Deepgram Nova-2 Medical STT provider.

use std::io::Cursor;
use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use reqwest::{Client, header};
use serde::Deserialize;
use tracing::instrument;

use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::{AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk, TranscriptSegment};

// ── WAV encoding ─────────────────────────────────────────────────────────────

/// Encode `AudioData` (f32 PCM) into an in-memory 16-bit PCM WAV file.
///
/// Uses 16-bit signed integer format for maximum STT provider compatibility.
/// The output is written with [`hound`] and can be shared across providers.
pub fn encode_audio_to_wav(audio: &AudioData) -> AppResult<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());

    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| AppError::SttProvider(format!("WAV init: {e}")))?;

        for &s in &audio.samples {
            // Clamp f32 [-1.0, 1.0] → i16 range and write as 16-bit PCM.
            let clamped = s.clamp(-1.0, 1.0);
            let sample_i16 = (clamped * i16::MAX as f32) as i16;
            writer
                .write_sample(sample_i16)
                .map_err(|e| AppError::SttProvider(format!("WAV write: {e}")))?;
        }

        writer
            .finalize()
            .map_err(|e| AppError::SttProvider(format!("WAV finalise: {e}")))?;
    }

    Ok(cursor.into_inner())
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    results: Option<DeepgramResults>,
}

#[derive(Debug, Deserialize)]
struct DeepgramResults {
    channels: Option<Vec<DeepgramChannel>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Option<Vec<DeepgramAlternative>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    transcript: Option<String>,
    words: Option<Vec<DeepgramWord>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramWord {
    word: Option<String>,
    start: Option<f64>,
    end: Option<f64>,
    confidence: Option<f64>,
    speaker: Option<u32>,
}

// ── Provider ─────────────────────────────────────────────────────────────────

/// Deepgram Nova-2 Medical speech-to-text provider.
pub struct DeepgramProvider {
    client: Client,
}

impl DeepgramProvider {
    /// Create a new provider with the given Deepgram API key.
    ///
    /// Uses "Authorization: Token {key}" authentication and a 300-second timeout.
    pub fn new(api_key: &str) -> AppResult<Self> {
        let mut auth_value =
            header::HeaderValue::from_str(&format!("Token {api_key}"))
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
impl SttProvider for DeepgramProvider {
    fn name(&self) -> &str {
        "deepgram"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_diarization(&self) -> bool {
        true
    }

    #[instrument(skip(self, audio, config), fields(provider = "deepgram"))]
    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        let wav_bytes = encode_audio_to_wav(&audio)?;
        let duration = audio.duration_seconds();

        // Build query parameters.
        let mut url = "https://api.deepgram.com/v1/listen?model=nova-2-medical&smart_format=true"
            .to_owned();

        if config.diarize {
            url.push_str("&diarize=true");
            if let Some(n) = config.num_speakers {
                url.push_str(&format!("&diarize_version=2&num_speakers={n}"));
            }
        }

        if let Some(ref lang) = config.language {
            url.push_str(&format!("&language={lang}"));
        }

        let response = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "audio/wav")
            .body(wav_bytes)
            .send()
            .await
            .map_err(|e| AppError::SttProvider(format!("Deepgram request: {e}")))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(AppError::SttProvider(format!(
                "Deepgram HTTP {status}: {body_text}"
            )));
        }

        tracing::debug!(
            response_len = body_text.len(),
            response_body = %body_text,
            "Deepgram raw response"
        );

        let dg: DeepgramResponse = serde_json::from_str(&body_text)
            .map_err(|e| AppError::SttProvider(format!("Deepgram JSON: {e}")))?;

        // Extract text and word-level segments.
        let alt = dg
            .results
            .as_ref()
            .and_then(|r| r.channels.as_ref())
            .and_then(|ch| ch.first())
            .and_then(|c| c.alternatives.as_ref())
            .and_then(|a| a.first());

        let text = alt
            .and_then(|a| a.transcript.clone())
            .unwrap_or_default();

        let segments: Vec<TranscriptSegment> = alt
            .and_then(|a| a.words.as_ref())
            .map(|words| {
                words
                    .iter()
                    .filter_map(|w| {
                        Some(TranscriptSegment {
                            text: w.word.clone()?,
                            start: w.start?,
                            end: w.end?,
                            speaker: w.speaker.map(|s| format!("speaker_{s}")),
                            confidence: w.confidence.map(|c| c as f32),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Transcript {
            text,
            segments,
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
            "deepgram: streaming not yet implemented".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_audio_produces_valid_wav() {
        let audio = AudioData {
            samples: vec![0.0f32; 1600],
            sample_rate: 16000,
            channels: 1,
        };
        let wav = encode_audio_to_wav(&audio).expect("WAV encoding failed");
        // Must start with RIFF and be larger than the 44-byte header.
        assert!(wav.len() > 44, "WAV too short: {} bytes", wav.len());
        assert_eq!(&wav[0..4], b"RIFF", "WAV header missing RIFF magic");
    }
}
