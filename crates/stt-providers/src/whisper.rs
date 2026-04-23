//! Whisper transcription via whisper-rs.

use std::path::PathBuf;

use tracing::{info, instrument};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use medical_core::error::{AppError, AppResult};

/// A timestamped segment from whisper transcription.
#[derive(Debug, Clone)]
pub struct WhisperSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Wrapper around whisper-rs for local transcription.
pub struct WhisperTranscriber {
    model_path: PathBuf,
}

impl WhisperTranscriber {
    pub fn new(model_path: PathBuf) -> Self {
        Self { model_path }
    }

    /// Transcribe 16 kHz mono f32 audio.
    /// Must be called on a blocking thread (or via `spawn_blocking`).
    #[instrument(skip(self, audio_16k_mono), fields(provider = "whisper_local"))]
    pub fn transcribe(
        &self,
        audio_16k_mono: &[f32],
        language: Option<&str>,
    ) -> AppResult<Vec<WhisperSegment>> {
        let ctx = WhisperContext::new_with_params(
            self.model_path.to_str().ok_or_else(|| {
                AppError::SttProvider("Model path is not valid UTF-8".into())
            })?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| AppError::SttProvider(format!("Failed to load Whisper model: {e}")))?;

        let mut state = ctx.create_state().map_err(|e| {
            AppError::SttProvider(format!("Failed to create Whisper state: {e}"))
        })?;

        // BeamSearch with beam_size=5 matches whisper.cpp's default and is
        // empirically ~3× more complete than Greedy{best_of=1} on long audio
        // with medical terminology — greedy decoding triggers whisper.cpp's
        // hallucination-skip on difficult stretches, silently dropping content.
        // See crates/stt-providers/examples/transcribe_probe.rs for the
        // A/B/C comparison that motivated this change.
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });

        let lang_code: Option<String> = language.map(|l| l.chars().take(2).collect());
        params.set_language(lang_code.as_deref());
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_translate(false);
        params.set_no_timestamps(false);
        // Temperature fallback breaks repetition loops: if a decoding attempt
        // looks degenerate, whisper.cpp retries with temperature += 0.2. These
        // are whisper.cpp's own default values.
        params.set_temperature(0.0);
        params.set_temperature_inc(0.2);

        info!(
            samples = audio_16k_mono.len(),
            duration_s = audio_16k_mono.len() as f64 / 16_000.0,
            "Running local Whisper inference"
        );

        state.full(params, audio_16k_mono).map_err(|e| {
            AppError::SttProvider(format!("Whisper inference failed: {e}"))
        })?;

        let num_segments = state.full_n_segments();
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

            let text_trimmed = text.trim().to_owned();
            if !text_trimmed.is_empty() {
                segments.push(WhisperSegment {
                    text: text_trimmed,
                    start,
                    end,
                });
            }
        }

        info!(segments = segments.len(), "Whisper transcription complete");
        Ok(segments)
    }
}
