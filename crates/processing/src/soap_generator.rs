//! SOAP note prompt generation.

use medical_core::types::settings::SoapTemplate;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for building a SOAP note system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapPromptConfig {
    pub template: SoapTemplate,
    pub icd_version: String,
    pub custom_prompt: Option<String>,
    pub include_context: bool,
}

impl Default for SoapPromptConfig {
    fn default() -> Self {
        Self {
            template: SoapTemplate::FollowUp,
            icd_version: "ICD-10".into(),
            custom_prompt: None,
            include_context: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt builders
// ---------------------------------------------------------------------------

/// Build the system prompt for SOAP note generation.
///
/// If `config.custom_prompt` is non-empty it is returned verbatim; otherwise a
/// template-specific prompt is generated.
pub fn build_soap_prompt(config: &SoapPromptConfig) -> String {
    // Use the custom prompt verbatim when provided.
    if let Some(ref custom) = config.custom_prompt
        && !custom.is_empty() {
            return custom.clone();
        }

    let template_instruction = match config.template {
        SoapTemplate::FollowUp => {
            "Focus on changes since last visit, interval history, and response to current \
             treatment plan."
        }
        SoapTemplate::NewPatient => {
            "Provide comprehensive history including past medical history, family history, \
             social history, and review of systems."
        }
        SoapTemplate::Telehealth => {
            "Note the limitations of remote examination. Document what was assessed virtually \
             and any elements requiring in-person follow-up."
        }
        SoapTemplate::Emergency => {
            "Prioritise acute findings. Document chief complaint, vital signs, acute \
             interventions, and disposition."
        }
        SoapTemplate::Pediatric => {
            "Include developmental milestones, immunisation status, growth parameters, and \
             age-appropriate screening."
        }
        SoapTemplate::Geriatric => {
            "Address functional status, fall risk assessment, polypharmacy review, cognitive \
             screening, and social support."
        }
    };

    format!(
        "You are a medical scribe assistant. Generate a structured SOAP note using \
         {icd_version} diagnosis codes.\n\n\
         Template guidance: {template_instruction}\n\n\
         Structure your response with clearly labelled sections:\n\
         - Subjective (S): Patient-reported symptoms, history, and complaints\n\
         - Objective (O): Measurable findings, vital signs, and examination results\n\
         - Assessment (A): Diagnosis with {icd_version} codes and clinical reasoning\n\
         - Plan (P): Treatment, medications, referrals, and follow-up\n\n\
         Be concise, clinically accurate, and use appropriate medical terminology.",
        icd_version = config.icd_version,
        template_instruction = template_instruction,
    )
}

/// Build the user-turn prompt containing the transcript (and optional context).
pub fn build_user_prompt(transcript: &str, context: Option<&str>) -> String {
    match context {
        Some(ctx) if !ctx.is_empty() => format!(
            "Additional context:\n{ctx}\n\nTranscript:\n{transcript}",
            ctx = ctx,
            transcript = transcript,
        ),
        _ => format!("Transcript:\n{}", transcript),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_includes_icd() {
        let config = SoapPromptConfig::default();
        let prompt = build_soap_prompt(&config);
        assert!(prompt.contains("ICD-10"));
        assert!(prompt.contains("Subjective"));
        assert!(prompt.contains("Objective"));
        assert!(prompt.contains("Assessment"));
        assert!(prompt.contains("Plan"));
    }

    #[test]
    fn custom_prompt_overrides() {
        let config = SoapPromptConfig {
            custom_prompt: Some("My custom prompt".into()),
            ..Default::default()
        };
        let prompt = build_soap_prompt(&config);
        assert_eq!(prompt, "My custom prompt");
    }

    #[test]
    fn template_specific_instructions() {
        let follow_up_config = SoapPromptConfig {
            template: SoapTemplate::FollowUp,
            ..Default::default()
        };
        assert!(build_soap_prompt(&follow_up_config).contains("changes since last visit"));

        let new_patient_config = SoapPromptConfig {
            template: SoapTemplate::NewPatient,
            ..Default::default()
        };
        assert!(build_soap_prompt(&new_patient_config).contains("comprehensive history"));

        let telehealth_config = SoapPromptConfig {
            template: SoapTemplate::Telehealth,
            ..Default::default()
        };
        assert!(build_soap_prompt(&telehealth_config).contains("limitations of remote"));

        let emergency_config = SoapPromptConfig {
            template: SoapTemplate::Emergency,
            ..Default::default()
        };
        assert!(build_soap_prompt(&emergency_config).contains("acute findings"));

        let pediatric_config = SoapPromptConfig {
            template: SoapTemplate::Pediatric,
            ..Default::default()
        };
        assert!(build_soap_prompt(&pediatric_config).contains("developmental milestones"));

        let geriatric_config = SoapPromptConfig {
            template: SoapTemplate::Geriatric,
            ..Default::default()
        };
        assert!(build_soap_prompt(&geriatric_config).contains("functional status"));
        assert!(build_soap_prompt(&geriatric_config).contains("fall risk"));
        assert!(build_soap_prompt(&geriatric_config).contains("polypharmacy"));
    }

    #[test]
    fn user_prompt_with_context() {
        let prompt = build_user_prompt("patient transcript", Some("prior visit notes"));
        assert!(prompt.contains("prior visit notes"));
        assert!(prompt.contains("patient transcript"));
        assert!(prompt.contains("Additional context"));
    }

    #[test]
    fn user_prompt_without_context() {
        let prompt = build_user_prompt("patient transcript", None);
        assert!(prompt.contains("patient transcript"));
        assert!(!prompt.contains("Additional context"));
    }
}
