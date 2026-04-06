//! Document prompt builders for referral letters, patient correspondence, and synopses.

// ---------------------------------------------------------------------------
// Referral letter
// ---------------------------------------------------------------------------

/// Build `(system_prompt, user_prompt)` for generating a referral letter.
///
/// # Arguments
/// * `soap_note`      – The SOAP note text to base the letter on.
/// * `recipient_type` – E.g. `"Cardiologist"`, `"Orthopaedics"`.
/// * `urgency`        – E.g. `"routine"`, `"urgent"`, `"emergency"`.
pub fn build_referral_prompt(
    soap_note: &str,
    recipient_type: &str,
    urgency: &str,
) -> (String, String) {
    let system = format!(
        "You are a medical scribe assistant specialising in professional referral letters. \
         Write a formal referral letter addressed to a {recipient_type}. \
         The urgency of this referral is: {urgency}. \
         Use appropriate clinical language, include relevant history and findings from the SOAP \
         note, clearly state the reason for referral, and request the desired action. \
         Format the letter professionally with greeting, body, and closing.",
        recipient_type = recipient_type,
        urgency = urgency,
    );

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
///
/// # Arguments
/// * `soap_note`   – The SOAP note to draw information from.
/// * `letter_type` – E.g. `"results"`, `"instructions"`, `"follow-up"`.
pub fn build_letter_prompt(soap_note: &str, letter_type: &str) -> (String, String) {
    let system = format!(
        "You are a medical scribe assistant helping to write patient-friendly correspondence. \
         Generate a {letter_type} letter for the patient. \
         Use clear, plain language the patient can understand. \
         Avoid unexplained medical jargon. \
         Be empathetic and professional.",
        letter_type = letter_type,
    );

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
///
/// The synopsis should be under 200 words.
pub fn build_synopsis_prompt(soap_note: &str) -> (String, String) {
    let system = "You are a medical scribe assistant. Summarise the provided SOAP note in a \
                  concise synopsis of no more than 200 words. \
                  Capture the key subjective complaints, objective findings, primary diagnosis, \
                  and treatment plan. \
                  Write in clear, professional language suitable for a quick clinical overview."
        .to_string();

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
    fn referral_includes_recipient_and_urgency() {
        let soap = "S: Chest pain\nO: BP 140/90\nA: Hypertension\nP: Refer to Cardiology";
        let (system, user) = build_referral_prompt(soap, "Cardiologist", "urgent");

        assert!(system.contains("Cardiologist"));
        assert!(system.contains("urgent"));
        assert!(user.contains("Cardiologist"));
        assert!(user.contains("urgent"));
        assert!(user.contains("Chest pain"));
    }

    #[test]
    fn letter_includes_type() {
        let soap = "S: Anxiety\nO: HR 90\nA: GAD\nP: CBT referral";
        let (system, user) = build_letter_prompt(soap, "results");

        assert!(system.contains("results"));
        assert!(user.contains("results"));
        assert!(user.contains("Anxiety"));
    }

    #[test]
    fn synopsis_word_limit() {
        let soap =
            "S: Patient reports fatigue\nO: Haemoglobin 9.0\nA: Iron deficiency anaemia\n\
             P: Ferrous sulfate 200 mg TDS for 3 months, recheck bloods in 4 weeks";
        let (system, user) = build_synopsis_prompt(soap);

        // System prompt must mention the 200-word limit.
        assert!(system.contains("200 words") || system.contains("200-word"));
        // User prompt contains the SOAP note.
        assert!(user.contains("Iron deficiency anaemia"));
        assert!(user.contains("200 words"));
    }
}
