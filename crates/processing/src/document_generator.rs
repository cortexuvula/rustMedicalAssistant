//! Prompt builders for referral letters, patient correspondence, and synopses.
//!
//! Each builder accepts an optional custom template override; placeholders
//! (`{recipient_type}`, `{urgency}`, `{letter_type}`) are resolved by
//! `prompt_resolver::resolve_prompt`.

use std::collections::HashMap;

use crate::prompt_resolver::resolve_prompt;

// ---------------------------------------------------------------------------
// Default templates
// ---------------------------------------------------------------------------

pub fn default_referral_prompt() -> &'static str {
    "You are a medical scribe assistant specialising in professional referral letters. \
     Write a formal referral letter addressed to a {recipient_type}. \
     The urgency of this referral is: {urgency}. \
     Use appropriate clinical language, include relevant history and findings from the SOAP \
     note, clearly state the reason for referral, and request the desired action. \
     Format the letter professionally with greeting, body, and closing."
}

pub fn default_letter_prompt() -> &'static str {
    "You are a medical scribe assistant helping to write patient-friendly correspondence. \
     Generate a {letter_type} letter for the patient. \
     Use clear, plain language the patient can understand. \
     Avoid unexplained medical jargon. \
     Be empathetic and professional."
}

pub fn default_synopsis_prompt() -> &'static str {
    "You are a medical scribe assistant. Summarise the provided SOAP note in a \
     concise synopsis of no more than 200 words. \
     Capture the key subjective complaints, objective findings, primary diagnosis, \
     and treatment plan. \
     Write in clear, professional language suitable for a quick clinical overview."
}

// ---------------------------------------------------------------------------
// Referral letter
// ---------------------------------------------------------------------------

/// Build `(system_prompt, user_prompt)` for generating a referral letter.
pub fn build_referral_prompt(
    soap_note: &str,
    recipient_type: &str,
    urgency: &str,
    custom_template: Option<&str>,
) -> (String, String) {
    let template = custom_template
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_referral_prompt());

    let mut placeholders = HashMap::new();
    placeholders.insert("recipient_type", recipient_type.to_string());
    placeholders.insert("urgency", urgency.to_string());

    let system = resolve_prompt(template, &placeholders);

    let user = format!(
        "Please write a referral letter to a {recipient_type} with {urgency} urgency based on \
         the following SOAP note:\n\n{soap_note}",
        recipient_type = recipient_type,
        urgency = urgency,
        soap_note = soap_note,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Patient letter
// ---------------------------------------------------------------------------

/// Build `(system_prompt, user_prompt)` for generating patient correspondence.
pub fn build_letter_prompt(
    soap_note: &str,
    letter_type: &str,
    custom_template: Option<&str>,
) -> (String, String) {
    let template = custom_template
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_letter_prompt());

    let mut placeholders = HashMap::new();
    placeholders.insert("letter_type", letter_type.to_string());

    let system = resolve_prompt(template, &placeholders);

    let user = format!(
        "Please write a {letter_type} letter for the patient based on the following SOAP \
         note:\n\n{soap_note}",
        letter_type = letter_type,
        soap_note = soap_note,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Synopsis
// ---------------------------------------------------------------------------

/// Build `(system_prompt, user_prompt)` for generating a brief SOAP synopsis.
pub fn build_synopsis_prompt(
    soap_note: &str,
    custom_template: Option<&str>,
) -> (String, String) {
    let template = custom_template
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_synopsis_prompt());

    // Synopsis template has no placeholders; pass empty map.
    let system = resolve_prompt(template, &HashMap::new());

    let user = format!(
        "Please summarise the following SOAP note in under 200 words:\n\n{soap_note}",
        soap_note = soap_note,
    );

    (system, user)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referral_default_contains_recipient_and_urgency() {
        let soap = "S: Chest pain\nO: BP 140/90\nA: Hypertension\nP: Refer to Cardiology";
        let (system, user) = build_referral_prompt(soap, "Cardiologist", "urgent", None);

        assert!(system.contains("Cardiologist"));
        assert!(system.contains("urgent"));
        assert!(!system.contains("{recipient_type}"));
        assert!(!system.contains("{urgency}"));
        assert!(user.contains("Chest pain"));
    }

    #[test]
    fn referral_custom_template_overrides() {
        let soap = "S: foo";
        let custom = "CUSTOM: Refer to {recipient_type} ({urgency})";
        let (system, _user) = build_referral_prompt(soap, "Neurology", "routine", Some(custom));

        assert!(system.starts_with("CUSTOM: Refer to Neurology (routine)"));
    }

    #[test]
    fn referral_empty_custom_falls_back_to_default() {
        let soap = "S: foo";
        let (system, _user) = build_referral_prompt(soap, "Derm", "routine", Some(""));
        assert!(system.contains("professional referral letters"));
    }

    #[test]
    fn letter_default_contains_type() {
        let soap = "S: Anxiety\nO: HR 90\nA: GAD\nP: CBT referral";
        let (system, user) = build_letter_prompt(soap, "results", None);

        assert!(system.contains("results"));
        assert!(!system.contains("{letter_type}"));
        assert!(user.contains("Anxiety"));
    }

    #[test]
    fn letter_custom_template_overrides() {
        let soap = "S: foo";
        let custom = "CUSTOM: {letter_type} letter";
        let (system, _user) = build_letter_prompt(soap, "follow-up", Some(custom));
        assert!(system.starts_with("CUSTOM: follow-up letter"));
    }

    #[test]
    fn synopsis_default_mentions_word_limit() {
        let soap = "S: Patient reports fatigue\nO: Haemoglobin 9.0\nA: Iron deficiency anaemia";
        let (system, user) = build_synopsis_prompt(soap, None);
        assert!(system.contains("200 words") || system.contains("200-word"));
        assert!(user.contains("Iron deficiency anaemia"));
    }

    #[test]
    fn synopsis_custom_template_overrides() {
        let soap = "S: foo";
        let (system, _user) = build_synopsis_prompt(soap, Some("CUSTOM SYNOPSIS"));
        assert!(system.starts_with("CUSTOM SYNOPSIS"));
    }
}
