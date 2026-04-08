//! Local Whisper STT via whisper-rs. Feature-gated behind `local-stt`.
//!
//! When the `local-stt` feature is enabled, this module provides a fully
//! functional [`WhisperLocalProvider`] that runs whisper.cpp inference locally
//! using the `whisper-rs` crate.  When the feature is **disabled** (the
//! default), a zero-sized stub is provided so downstream code can still name
//! the type without pulling in the native dependency.

// ── Feature-gated implementation ────────────────────────────────────────────

#[cfg(feature = "local-stt")]
mod inner {
    use std::path::PathBuf;

    use async_trait::async_trait;
    use futures_core::Stream;
    use tracing::{info, instrument};
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    use medical_core::error::{AppError, AppResult};
    use medical_core::traits::SttProvider;
    use medical_core::types::{
        AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk, TranscriptSegment,
    };

    /// Local Whisper speech-to-text provider backed by whisper.cpp.
    ///
    /// Requires a GGML model file on disk (e.g. `ggml-base.en.bin`).  Models
    /// can be downloaded from
    /// <https://huggingface.co/ggerganov/whisper.cpp/tree/main>.
    pub struct WhisperLocalProvider {
        model_path: PathBuf,
    }

    impl WhisperLocalProvider {
        /// Create a new provider pointing at the given GGML model file.
        ///
        /// Returns an error if the file does not exist on disk.
        pub fn new(model_path: PathBuf) -> AppResult<Self> {
            if !model_path.exists() {
                return Err(AppError::SttProvider(format!(
                    "Whisper model not found at {}. Download a ggml model to this path.",
                    model_path.display()
                )));
            }
            Ok(Self { model_path })
        }
    }

    #[async_trait]
    impl SttProvider for WhisperLocalProvider {
        fn name(&self) -> &str {
            "whisper_local"
        }

        fn supports_streaming(&self) -> bool {
            false
        }

        fn supports_diarization(&self) -> bool {
            false
        }

        #[instrument(skip(self, audio, config), fields(provider = "whisper_local"))]
        async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
            let model_path = self.model_path.clone();
            let duration = audio.duration_seconds();

            tokio::task::spawn_blocking(move || {
                // ── Build context ────────────────────────────────────────
                let ctx = WhisperContext::new_with_params(
                    model_path.to_str().ok_or_else(|| {
                        AppError::SttProvider("Model path is not valid UTF-8".into())
                    })?,
                    WhisperContextParameters::default(),
                )
                .map_err(|e| {
                    AppError::SttProvider(format!("Failed to load Whisper model: {e}"))
                })?;

                let mut state = ctx.create_state().map_err(|e| {
                    AppError::SttProvider(format!("Failed to create Whisper state: {e}"))
                })?;

                // ── Configure parameters ─────────────────────────────────
                let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

                // Language hint (ISO-639-1 two-letter code).
                // Must live as long as `params` since set_language borrows it.
                let lang_code: Option<String> = config
                    .language
                    .as_ref()
                    .map(|l| l.chars().take(2).collect());

                params.set_language(lang_code.as_deref());

                params.set_print_special(false);
                params.set_print_progress(false);
                params.set_print_realtime(false);
                params.set_print_timestamps(false);
                params.set_translate(false);
                params.set_no_timestamps(false);

                // ── Resample to 16 kHz mono if necessary ─────────────────
                let samples_16k: Vec<f32> = if audio.sample_rate != 16000 || audio.channels != 1 {
                    resample_to_16k_mono(&audio)
                } else {
                    audio.samples.clone()
                };

                info!(
                    samples = samples_16k.len(),
                    duration_s = duration,
                    "Running local Whisper inference"
                );

                // ── Run inference ────────────────────────────────────────
                state.full(params, &samples_16k).map_err(|e| {
                    AppError::SttProvider(format!("Whisper inference failed: {e}"))
                })?;

                // ── Extract segments ─────────────────────────────────────
                let num_segments = state.full_n_segments();

                let mut full_text = String::new();
                let mut segments = Vec::with_capacity(num_segments as usize);

                for i in 0..num_segments {
                    let segment = state.get_segment(i).ok_or_else(|| {
                        AppError::SttProvider(format!("Segment {i} out of bounds"))
                    })?;

                    let text = segment.to_str_lossy().map_err(|e| {
                        AppError::SttProvider(format!("Failed to get segment {i} text: {e}"))
                    })?;

                    // whisper.cpp timestamps are in centiseconds.
                    let start = segment.start_timestamp() as f64 / 100.0;
                    let end = segment.end_timestamp() as f64 / 100.0;

                    let text_trimmed = text.trim();

                    if !full_text.is_empty() && !text_trimmed.is_empty() {
                        full_text.push(' ');
                    }
                    full_text.push_str(text_trimmed);

                    segments.push(TranscriptSegment {
                        text: text_trimmed.to_owned(),
                        start,
                        end,
                        speaker: None,
                        confidence: None,
                    });
                }

                Ok(Transcript {
                    text: full_text,
                    segments,
                    language: config.language.clone(),
                    duration_seconds: Some(duration),
                    provider: "whisper_local".to_owned(),
                    metadata: serde_json::json!({
                        "model_path": model_path.display().to_string(),
                    }),
                })
            })
            .await
            .map_err(|e| AppError::SttProvider(format!("Whisper task panicked: {e}")))?
        }

        async fn transcribe_stream(
            &self,
            _stream: AudioStream,
            _config: SttConfig,
        ) -> AppResult<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send + Unpin>> {
            Err(AppError::SttProvider(
                "whisper_local: streaming transcription is not supported".to_owned(),
            ))
        }
    }

    /// Naively resample multi-channel / non-16 kHz audio to 16 kHz mono.
    ///
    /// This uses simple linear interpolation which is good enough for speech.
    /// For production quality consider a proper resampler (e.g. rubato).
    fn resample_to_16k_mono(audio: &AudioData) -> Vec<f32> {
        let channels = audio.channels.max(1) as usize;

        // First, mix down to mono by averaging channels.
        let mono: Vec<f32> = if channels > 1 {
            audio
                .samples
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            audio.samples.clone()
        };

        let src_rate = audio.sample_rate as f64;
        let dst_rate = 16000.0_f64;

        if (src_rate - dst_rate).abs() < 1.0 {
            return mono;
        }

        let ratio = src_rate / dst_rate;
        let out_len = (mono.len() as f64 / ratio).ceil() as usize;
        let mut out = Vec::with_capacity(out_len);

        for i in 0..out_len {
            let src_idx = i as f64 * ratio;
            let idx0 = src_idx.floor() as usize;
            let idx1 = (idx0 + 1).min(mono.len().saturating_sub(1));
            let frac = (src_idx - idx0 as f64) as f32;
            out.push(mono[idx0] * (1.0 - frac) + mono[idx1] * frac);
        }

        out
    }
}

#[cfg(feature = "local-stt")]
pub use inner::WhisperLocalProvider;

// ── Stub when feature is disabled ────────────────────────────────────────────

#[cfg(not(feature = "local-stt"))]
pub struct WhisperLocalProvider;

#[cfg(not(feature = "local-stt"))]
impl WhisperLocalProvider {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(feature = "local-stt"))]
impl Default for WhisperLocalProvider {
    fn default() -> Self {
        Self::new()
    }
}
