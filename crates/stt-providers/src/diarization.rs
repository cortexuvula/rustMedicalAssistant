//! Speaker diarization module.
//!
//! Currently stubbed — pyannote-rs v0.3.4 has an ndarray version conflict
//! (0.16 vs 0.17) preventing compilation. When a working diarization crate
//! is available (speakrs recommended for best quality), implement the
//! `diarize()` method to call the real pipeline.
//!
//! The rest of the codebase handles empty diarization results gracefully —
//! transcripts are returned without speaker labels.

use std::path::PathBuf;

use tracing::{info, warn};

use medical_core::error::AppResult;

/// A speaker turn: a contiguous time range attributed to one speaker.
#[derive(Debug, Clone)]
pub struct SpeakerTurn {
    pub speaker_id: usize,
    pub start: f64,
    pub end: f64,
}

/// Speaker diarization using ONNX models.
///
/// Currently a no-op stub. When a working diarization crate is available,
/// this struct will hold model paths and run the real pipeline.
pub struct SpeakerDiarizer {
    _segmentation_path: PathBuf,
    _embedding_path: PathBuf,
}

impl SpeakerDiarizer {
    pub fn new(segmentation_path: PathBuf, embedding_path: PathBuf) -> Self {
        Self {
            _segmentation_path: segmentation_path,
            _embedding_path: embedding_path,
        }
    }

    /// Run speaker diarization on 16 kHz mono audio.
    ///
    /// Currently returns empty results (stub). The caller (LocalSttProvider)
    /// handles this gracefully by producing transcripts without speaker labels.
    pub fn diarize(
        &self,
        _samples_i16: &[i16],
        _sample_rate: u32,
    ) -> AppResult<Vec<SpeakerTurn>> {
        warn!(
            "Speaker diarization is not yet available (pyannote-rs dependency issue). \
             Returning transcript without speaker labels."
        );
        info!(
            "To enable diarization, replace the stub in diarization.rs with a working \
             crate (speakrs recommended for best accuracy)."
        );
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn stub_returns_empty() {
        let diarizer = SpeakerDiarizer::new(
            PathBuf::from("/nonexistent/seg.onnx"),
            PathBuf::from("/nonexistent/emb.onnx"),
        );
        let result = diarizer.diarize(&[0i16; 16000], 16000).unwrap();
        assert!(result.is_empty());
    }
}
