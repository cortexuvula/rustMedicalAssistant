use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::agent::AgentSettings;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// UI theme preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Theme {
    Dark,
    #[default]
    Light,
}


/// ICD coding version used when generating SOAP notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum IcdVersion {
    #[default]
    Icd9,
    Icd10,
    Both,
}


/// The SOAP note template style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SoapTemplate {
    #[default]
    FollowUp,
    NewPatient,
    Telehealth,
    Emergency,
    Pediatric,
    Geriatric,
}


// ---------------------------------------------------------------------------
// SoapNoteSettings
// ---------------------------------------------------------------------------

/// Per-provider model overrides and ICD coding settings for SOAP generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapNoteSettings {
    pub openai_model: String,
    pub anthropic_model: String,
    pub groq_model: String,
    pub icd_code_version: IcdVersion,
}

impl Default for SoapNoteSettings {
    fn default() -> Self {
        Self {
            openai_model: "gpt-4o".into(),
            anthropic_model: "claude-3-5-sonnet-20241022".into(),
            groq_model: "llama-3.1-70b-versatile".into(),
            icd_code_version: IcdVersion::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Default helpers
// ---------------------------------------------------------------------------

fn default_language() -> String {
    "en-US".into()
}

fn default_sample_rate() -> u32 {
    44100
}

fn default_channels() -> u16 {
    1
}

fn default_ai_provider() -> String {
    "openai".into()
}

fn default_ai_model() -> String {
    "gpt-4o".into()
}

fn default_whisper_model() -> String {
    "large-v3-turbo".into()
}

fn default_tts_provider() -> String {
    "elevenlabs".into()
}

fn default_tts_voice() -> String {
    "default".into()
}

fn default_temperature() -> f32 {
    0.2
}

fn default_icd_version() -> IcdVersion {
    IcdVersion::default()
}

fn default_soap_template() -> SoapTemplate {
    SoapTemplate::default()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".into()
}

fn default_search_top_k() -> u32 {
    5
}

fn default_mmr_lambda() -> f32 {
    0.7
}

fn default_autosave_enabled() -> bool {
    true
}

fn default_autosave_interval_secs() -> u64 {
    60
}

fn default_quick_continue_mode() -> bool {
    true
}

fn default_max_background_workers() -> u32 {
    2
}

fn default_show_processing_notifications() -> bool {
    true
}

fn default_auto_retry_failed() -> bool {
    true
}

fn default_auto_generate_soap() -> bool {
    false
}

fn default_max_retry_attempts() -> u32 {
    3
}

fn default_window_width() -> u32 {
    1200
}

fn default_window_height() -> u32 {
    800
}

fn default_auto_index_rag() -> bool {
    true
}

fn default_lmstudio_host() -> String {
    "localhost".into()
}

fn default_lmstudio_port() -> u16 {
    1234
}

fn default_vocabulary_enabled() -> bool {
    true
}

// ---------------------------------------------------------------------------
// AppConfig
// ---------------------------------------------------------------------------

/// The full application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // General
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub storage_path: Option<String>,

    // Audio
    #[serde(default)]
    pub input_device: Option<String>,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_channels")]
    pub channels: u16,

    // Providers
    #[serde(default = "default_ai_provider")]
    pub ai_provider: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    #[serde(default = "default_whisper_model")]
    pub whisper_model: String,
    #[serde(default = "default_tts_provider")]
    pub tts_provider: String,
    #[serde(default = "default_tts_voice")]
    pub tts_voice: String,
    // LM Studio remote server
    #[serde(default = "default_lmstudio_host")]
    pub lmstudio_host: String,
    #[serde(default = "default_lmstudio_port")]
    pub lmstudio_port: u16,

    // Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    // Processing
    #[serde(default)]
    pub auto_generate_referral: bool,
    #[serde(default)]
    pub auto_generate_letter: bool,
    #[serde(default = "default_auto_generate_soap")]
    pub auto_generate_soap: bool,
    #[serde(default = "default_auto_index_rag")]
    pub auto_index_rag: bool,
    #[serde(default = "default_vocabulary_enabled")]
    pub vocabulary_enabled: bool,
    #[serde(default = "default_icd_version")]
    pub icd_version: IcdVersion,

    // Templates
    #[serde(default = "default_soap_template")]
    pub soap_template: SoapTemplate,
    #[serde(default)]
    pub custom_soap_prompt: Option<String>,
    #[serde(default)]
    pub custom_referral_prompt: Option<String>,
    #[serde(default)]
    pub custom_letter_prompt: Option<String>,

    // RAG
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_search_top_k")]
    pub search_top_k: u32,
    #[serde(default = "default_mmr_lambda")]
    pub mmr_lambda: f32,

    // Autosave
    #[serde(default = "default_autosave_enabled")]
    pub autosave_enabled: bool,
    #[serde(default = "default_autosave_interval_secs")]
    pub autosave_interval_secs: u64,

    // Features
    #[serde(default = "default_quick_continue_mode")]
    pub quick_continue_mode: bool,
    #[serde(default = "default_max_background_workers")]
    pub max_background_workers: u32,
    #[serde(default = "default_show_processing_notifications")]
    pub show_processing_notifications: bool,
    #[serde(default = "default_auto_retry_failed")]
    pub auto_retry_failed: bool,
    #[serde(default = "default_max_retry_attempts")]
    pub max_retry_attempts: u32,

    // Window
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_window_height")]
    pub window_height: u32,

    // Sub-configs
    #[serde(default)]
    pub soap_note_settings: SoapNoteSettings,
    #[serde(default)]
    pub agent_settings: HashMap<String, AgentSettings>,
}

impl Default for AppConfig {
    fn default() -> Self {
        serde_json::from_str("{}").expect("AppConfig default deserialization should never fail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = AppConfig::default();
        assert_eq!(config.theme, Theme::Light);
        assert_eq!(config.language, "en-US");
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 1);
        assert_eq!(config.ai_provider, "openai");
        assert_eq!(config.ai_model, "gpt-4o");
        assert_eq!(config.whisper_model, "large-v3-turbo");
        assert_eq!(config.tts_provider, "elevenlabs");
        assert_eq!(config.tts_voice, "default");
        assert!((config.temperature - 0.2).abs() < f32::EPSILON);
        assert!(!config.auto_generate_referral);
        assert!(!config.auto_generate_letter);
        assert!(!config.auto_generate_soap);
        assert!(config.auto_index_rag);
        assert_eq!(config.icd_version, IcdVersion::Icd9);
        assert_eq!(config.soap_template, SoapTemplate::FollowUp);
        assert_eq!(config.embedding_model, "text-embedding-3-small");
        assert_eq!(config.search_top_k, 5);
        assert!((config.mmr_lambda - 0.7).abs() < f32::EPSILON);
        assert!(config.autosave_enabled);
        assert_eq!(config.autosave_interval_secs, 60);
        assert!(config.quick_continue_mode);
        assert_eq!(config.max_background_workers, 2);
        assert!(config.show_processing_notifications);
        assert!(config.auto_retry_failed);
        assert_eq!(config.max_retry_attempts, 3);
        assert_eq!(config.window_width, 1200);
        assert_eq!(config.window_height, 800);
        assert_eq!(config.lmstudio_host, "localhost");
        assert_eq!(config.lmstudio_port, 1234);
        assert!(config.vocabulary_enabled);
    }

    #[test]
    fn round_trip_json() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ai_provider, config.ai_provider);
        assert_eq!(back.theme, config.theme);
        assert_eq!(back.window_width, config.window_width);
    }

    #[test]
    fn partial_json_deserialize() {
        // Only override a few fields; all others should use defaults.
        let json = r#"{"ai_provider": "anthropic", "window_width": 1920}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.ai_provider, "anthropic");
        assert_eq!(config.window_width, 1920);
        // Defaults still in place
        assert_eq!(config.language, "en-US");
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.tts_provider, "elevenlabs");
    }

    #[test]
    fn theme_serialization() {
        let dark = Theme::Dark;
        let json = serde_json::to_value(&dark).unwrap();
        assert_eq!(json, "dark");

        let light: Theme = serde_json::from_str("\"light\"").unwrap();
        assert_eq!(light, Theme::Light);
    }

    #[test]
    fn icd_version_serialization() {
        let icd10 = IcdVersion::Icd10;
        let json = serde_json::to_value(&icd10).unwrap();
        assert_eq!(json, "icd10");

        let both: IcdVersion = serde_json::from_str("\"both\"").unwrap();
        assert_eq!(both, IcdVersion::Both);
    }

    #[test]
    fn soap_template_serialization() {
        let template = SoapTemplate::NewPatient;
        let json = serde_json::to_value(&template).unwrap();
        assert_eq!(json, "new_patient");

        let telehealth: SoapTemplate = serde_json::from_str("\"telehealth\"").unwrap();
        assert_eq!(telehealth, SoapTemplate::Telehealth);
    }

}
