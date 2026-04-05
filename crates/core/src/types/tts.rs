use serde::{Deserialize, Serialize};

/// Configuration for a text-to-speech synthesis request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub voice: Option<String>,
    pub language: Option<String>,
    pub speed: f32,
    pub volume: f32,
    pub model: Option<String>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            voice: None,
            language: None,
            speed: 1.0,
            volume: 1.0,
            model: None,
        }
    }
}

/// Metadata about an available TTS voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: Option<String>,
    pub gender: Option<String>,
    pub preview_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tts_config_defaults() {
        let config = TtsConfig::default();
        assert!(config.voice.is_none());
        assert!(config.language.is_none());
        assert!((config.speed - 1.0).abs() < f32::EPSILON);
        assert!((config.volume - 1.0).abs() < f32::EPSILON);
        assert!(config.model.is_none());
    }

    #[test]
    fn tts_config_round_trip() {
        let config = TtsConfig {
            voice: Some("nova".into()),
            language: Some("en-US".into()),
            speed: 1.25,
            volume: 0.8,
            model: Some("eleven_monolingual_v1".into()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: TtsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.voice.as_deref(), Some("nova"));
        assert!((back.speed - 1.25).abs() < f32::EPSILON);
    }
}
