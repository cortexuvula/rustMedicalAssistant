use serde::{Deserialize, Serialize};

/// Raw PCM audio data ready for transcription.
#[derive(Debug, Clone)]
pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioData {
    /// Returns the duration of the audio in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / (self.sample_rate as f64 * self.channels as f64)
    }
}

/// Configuration for a speech-to-text request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub language: Option<String>,
    pub diarize: bool,
    pub num_speakers: Option<u32>,
    pub model: Option<String>,
    pub smart_formatting: bool,
    pub profanity_filter: bool,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            language: None,
            diarize: false,
            num_speakers: None,
            model: None,
            smart_formatting: true,
            profanity_filter: false,
        }
    }
}

/// A completed transcription result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
    pub language: Option<String>,
    pub duration_seconds: Option<f64>,
    pub provider: String,
    pub metadata: serde_json::Value,
}

/// A timed segment within a transcript, optionally attributed to a speaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub confidence: Option<f32>,
}

/// A streaming chunk from a real-time transcription session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptChunk {
    pub text: String,
    pub is_final: bool,
    pub speaker: Option<String>,
}

/// A stream of raw PCM audio frames.
pub type AudioStream = tokio::sync::mpsc::Receiver<Vec<f32>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_seconds_calculated_correctly() {
        let audio = AudioData {
            samples: vec![0.0f32; 44100],
            sample_rate: 44100,
            channels: 1,
        };
        let duration = audio.duration_seconds();
        assert!((duration - 1.0).abs() < 1e-6, "expected ~1.0s, got {duration}");
    }

    #[test]
    fn duration_seconds_stereo() {
        let audio = AudioData {
            samples: vec![0.0f32; 88200],
            sample_rate: 44100,
            channels: 2,
        };
        let duration = audio.duration_seconds();
        assert!((duration - 1.0).abs() < 1e-6, "expected ~1.0s, got {duration}");
    }

    #[test]
    fn duration_zero_guard() {
        let audio = AudioData {
            samples: vec![],
            sample_rate: 0,
            channels: 0,
        };
        assert_eq!(audio.duration_seconds(), 0.0);
    }

    #[test]
    fn stt_config_defaults() {
        let config = SttConfig::default();
        assert!(config.language.is_none());
        assert!(!config.diarize);
        assert!(config.num_speakers.is_none());
        assert!(config.model.is_none());
        assert!(config.smart_formatting);
        assert!(!config.profanity_filter);
    }
}
