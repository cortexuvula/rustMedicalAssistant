//! Local platform TTS provider.
//! Uses platform-native speech synthesis:
//!   - Linux: speech-dispatcher
//!   - macOS: NSSpeechSynthesizer
//!   - Windows: SAPI
//! Full implementation in a future plan.

pub struct LocalTtsProvider;

impl LocalTtsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalTtsProvider {
    fn default() -> Self {
        Self::new()
    }
}
