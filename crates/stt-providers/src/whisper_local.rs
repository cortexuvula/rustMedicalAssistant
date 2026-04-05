//! Local Whisper STT via whisper-rs. Feature-gated.
pub struct WhisperLocalProvider;
impl WhisperLocalProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WhisperLocalProvider {
    fn default() -> Self {
        Self::new()
    }
}
