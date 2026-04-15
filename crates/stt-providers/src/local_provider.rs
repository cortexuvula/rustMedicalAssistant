//! LocalSttProvider — the single SttProvider implementation for local inference.
//!
//! Orchestrates the two-stage pipeline:
//! 1. Whisper transcription (whisper-rs, Metal GPU)
//! 2. Pyannote speaker diarization (currently stubbed)
//! 3. Merge segments with speaker labels

use std::path::PathBuf;

use async_trait::async_trait;
use futures_core::Stream;
use tracing::{info, warn};

use medical_core::error::{AppError, AppResult};
use medical_core::traits::SttProvider;
use medical_core::types::{
    AudioData, AudioStream, SttConfig, Transcript, TranscriptChunk, TranscriptSegment,
};

use crate::audio_prep;
use crate::diarization::SpeakerDiarizer;
use crate::merge;
use crate::whisper::WhisperTranscriber;

/// Local speech-to-text provider using whisper-rs + diarization.
pub struct LocalSttProvider {
    whisper_model_path: PathBuf,
    segmentation_model_path: PathBuf,
    embedding_model_path: PathBuf,
}

impl LocalSttProvider {
    pub fn new(
        whisper_model_path: PathBuf,
        segmentation_model_path: PathBuf,
        embedding_model_path: PathBuf,
    ) -> Self {
        Self {
            whisper_model_path,
            segmentation_model_path,
            embedding_model_path,
        }
    }
}

#[async_trait]
impl SttProvider for LocalSttProvider {
    fn name(&self) -> &str {
        "local"
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_diarization(&self) -> bool {
        self.segmentation_model_path.exists() && self.embedding_model_path.exists()
    }

    async fn transcribe(&self, audio: AudioData, config: SttConfig) -> AppResult<Transcript> {
        if !self.whisper_model_path.exists() {
            return Err(AppError::SttProvider(format!(
                "Whisper model not found at {}. Download a model in Settings → Audio / STT.",
                self.whisper_model_path.display()
            )));
        }

        let duration = audio.duration_seconds();

        // Stage 1: Resample to 16kHz mono
        let audio_16k = audio_prep::to_16k_mono_f32(&audio);

        // Stage 2: Whisper transcription
        let whisper_path = self.whisper_model_path.clone();
        let language = config.language.clone();
        let audio_for_whisper = audio_16k.clone();

        let whisper_segments = tokio::task::spawn_blocking(move || {
            let transcriber = WhisperTranscriber::new(whisper_path);
            transcriber.transcribe(&audio_for_whisper, language.as_deref())
        })
        .await
        .map_err(|e| AppError::SttProvider(format!("Whisper task panicked: {e}")))?
        ?;

        // Stage 3: Speaker diarization (optional, currently stubbed)
        let speaker_turns = if config.diarize && self.supports_diarization() {
            let seg_path = self.segmentation_model_path.clone();
            let emb_path = self.embedding_model_path.clone();
            let audio_i16 = audio_prep::f32_to_i16(&audio_16k);

            match tokio::task::spawn_blocking(move || {
                let diarizer = SpeakerDiarizer::new(seg_path, emb_path);
                diarizer.diarize(&audio_i16, 16000)
            })
            .await
            {
                Ok(Ok(turns)) => turns,
                Ok(Err(e)) => {
                    warn!(error = %e, "Diarization failed — proceeding without speaker labels");
                    Vec::new()
                }
                Err(e) => {
                    warn!(error = %e, "Diarization task panicked — proceeding without speaker labels");
                    Vec::new()
                }
            }
        } else {
            if config.diarize && !self.supports_diarization() {
                warn!("Diarization requested but models not found — skipping");
            }
            Vec::new()
        };

        // Stage 4: Merge whisper segments with speaker turns
        let segments: Vec<TranscriptSegment> =
            merge::merge_segments_with_speakers(&whisper_segments, &speaker_turns);

        let full_text: String = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        info!(
            segments = segments.len(),
            text_len = full_text.len(),
            "Local transcription complete"
        );

        Ok(Transcript {
            text: full_text,
            segments,
            language: config.language.clone(),
            duration_seconds: Some(duration),
            provider: "local".to_owned(),
            metadata: serde_json::json!({
                "whisper_model": self.whisper_model_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown"),
                "diarization": !speaker_turns.is_empty(),
            }),
        })
    }

    async fn transcribe_stream(
        &self,
        _stream: AudioStream,
        _config: SttConfig,
    ) -> AppResult<Box<dyn Stream<Item = AppResult<TranscriptChunk>> + Send + Unpin>> {
        Err(AppError::SttProvider(
            "Local provider does not support streaming transcription".to_owned(),
        ))
    }
}
