//! Modulate/Velma STT — requires API credentials.
//! Features: emotion detection, diarization, deepfake detection, PII redaction.
pub struct ModulateProvider;
impl ModulateProvider {
    pub fn new(_api_key: &str) -> Self {
        Self
    }
}
