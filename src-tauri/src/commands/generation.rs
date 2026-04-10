//! Tauri commands for AI-powered document generation (SOAP, referral, letter, synopsis).

use std::sync::Arc;

use serde::Serialize;
use tauri::Emitter;
use tracing::debug;
use uuid::Uuid;

use medical_core::traits::AiProvider;
use medical_core::types::recording::Recording;
use medical_core::types::{CompletionRequest, Message, MessageContent, Role};
use medical_db::recordings::RecordingsRepo;
use medical_processing::document_generator;
use medical_processing::soap_generator::{self, SoapPromptConfig};
use medical_core::types::settings::SoapTemplate;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Progress event payload
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct GenerationProgress {
    #[serde(rename = "type")]
    doc_type: String,
    status: String,
    recording_id: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load a recording from DB and acquire the active AI provider in one step.
async fn get_provider_and_recording(
    state: &AppState,
    recording_id: &str,
) -> Result<(Arc<dyn AiProvider>, Recording), String> {
    let provider = {
        let registry = state.ai_providers.lock().await;
        registry
            .get_active_arc()
            .ok_or("No AI provider configured. Add an API key in Settings.")?
    };

    let uuid =
        Uuid::parse_str(recording_id).map_err(|e| format!("Invalid recording ID: {e}"))?;

    let recording = {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())?
    };

    Ok((provider, recording))
}

/// Parse a template string into the `SoapTemplate` enum.
fn parse_soap_template(s: &str) -> SoapTemplate {
    match s.to_lowercase().as_str() {
        "new_patient" | "newpatient" => SoapTemplate::NewPatient,
        "telehealth" => SoapTemplate::Telehealth,
        "emergency" => SoapTemplate::Emergency,
        "pediatric" => SoapTemplate::Pediatric,
        "geriatric" => SoapTemplate::Geriatric,
        _ => SoapTemplate::FollowUp, // default
    }
}

/// Load the user's AI model and temperature from saved settings.
fn load_ai_settings(state: &AppState) -> (String, f32) {
    let conn = state.db.conn().ok();
    let config = conn.and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok());
    match config {
        Some(cfg) => (cfg.ai_model, cfg.temperature),
        None => ("gpt-4o".to_string(), 0.4),
    }
}

/// Build a single-turn `CompletionRequest` from system and user prompts.
fn build_completion_request(
    system_prompt: String,
    user_prompt: String,
    model: String,
    temperature: f32,
) -> CompletionRequest {
    CompletionRequest {
        model,
        messages: vec![Message {
            role: Role::User,
            content: MessageContent::Text(user_prompt),
            tool_calls: vec![],
        }],
        temperature: Some(temperature),
        max_tokens: Some(4096),
        system_prompt: Some(system_prompt),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Generate a SOAP note from a recording's transcript.
///
/// Emits `generation-progress` events with `type: "soap"` and statuses
/// `"started"` / `"completed"` / `"failed"`.
#[tauri::command]
pub async fn generate_soap(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    template: Option<String>,
) -> Result<String, String> {
    // Emit: started
    let _ = app.emit(
        "generation-progress",
        GenerationProgress {
            doc_type: "soap".into(),
            status: "started".into(),
            recording_id: recording_id.clone(),
        },
    );

    let result = generate_soap_inner(&state, &recording_id, template.as_deref()).await;

    match &result {
        Ok(_) => {
            let _ = app.emit(
                "generation-progress",
                GenerationProgress {
                    doc_type: "soap".into(),
                    status: "completed".into(),
                    recording_id: recording_id.clone(),
                },
            );
        }
        Err(msg) => {
            let _ = app.emit(
                "generation-progress",
                GenerationProgress {
                    doc_type: "soap".into(),
                    status: format!("failed: {msg}"),
                    recording_id: recording_id.clone(),
                },
            );
        }
    }

    result
}

async fn generate_soap_inner(
    state: &AppState,
    recording_id: &str,
    template: Option<&str>,
) -> Result<String, String> {
    let (provider, mut recording) = get_provider_and_recording(state, recording_id).await?;

    let transcript = recording
        .transcript
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or("Recording has no transcript. Run transcription first.")?;

    // Build prompts
    let soap_template = template.map(parse_soap_template).unwrap_or_default();
    let config = SoapPromptConfig {
        template: soap_template,
        icd_version: "ICD-10".into(),
        custom_prompt: None,
        include_context: true,
    };

    let system_prompt = soap_generator::build_soap_prompt(&config);
    let user_prompt = soap_generator::build_user_prompt(transcript, None);

    debug!(
        "generate_soap: provider='{}', recording='{}'",
        provider.name(),
        recording_id,
    );

    let (model, temperature) = load_ai_settings(state);
    let request = build_completion_request(system_prompt, user_prompt, model, temperature);

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

    let soap_text = response.content;
    if soap_text.is_empty() {
        return Err("AI returned an empty SOAP note.".into());
    }

    // Persist to DB
    recording.soap_note = Some(soap_text.clone());
    {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())?;
    }

    Ok(soap_text)
}

/// Generate a referral letter from a recording's SOAP note.
///
/// Emits `generation-progress` events with `type: "referral"`.
#[tauri::command]
pub async fn generate_referral(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    recipient_type: Option<String>,
    urgency: Option<String>,
) -> Result<String, String> {
    let _ = app.emit(
        "generation-progress",
        GenerationProgress {
            doc_type: "referral".into(),
            status: "started".into(),
            recording_id: recording_id.clone(),
        },
    );

    let result = generate_referral_inner(
        &state,
        &recording_id,
        recipient_type.as_deref(),
        urgency.as_deref(),
    )
    .await;

    match &result {
        Ok(_) => {
            let _ = app.emit(
                "generation-progress",
                GenerationProgress {
                    doc_type: "referral".into(),
                    status: "completed".into(),
                    recording_id: recording_id.clone(),
                },
            );
        }
        Err(msg) => {
            let _ = app.emit(
                "generation-progress",
                GenerationProgress {
                    doc_type: "referral".into(),
                    status: format!("failed: {msg}"),
                    recording_id: recording_id.clone(),
                },
            );
        }
    }

    result
}

async fn generate_referral_inner(
    state: &AppState,
    recording_id: &str,
    recipient_type: Option<&str>,
    urgency: Option<&str>,
) -> Result<String, String> {
    let (provider, mut recording) = get_provider_and_recording(state, recording_id).await?;

    let soap_note = recording
        .soap_note
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Recording has no SOAP note. Generate a SOAP note first.")?;

    let recipient = recipient_type.unwrap_or("Specialist");
    let urg = urgency.unwrap_or("routine");

    let (system_prompt, user_prompt) =
        document_generator::build_referral_prompt(soap_note, recipient, urg);

    debug!(
        "generate_referral: provider='{}', recording='{}', recipient='{}', urgency='{}'",
        provider.name(),
        recording_id,
        recipient,
        urg,
    );

    let (model, temperature) = load_ai_settings(state);
    let request = build_completion_request(system_prompt, user_prompt, model, temperature);

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

    let referral_text = response.content;
    if referral_text.is_empty() {
        return Err("AI returned an empty referral letter.".into());
    }

    // Persist to DB
    recording.referral = Some(referral_text.clone());
    {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())?;
    }

    Ok(referral_text)
}

/// Generate a patient letter from a recording's SOAP note.
///
/// Emits `generation-progress` events with `type: "letter"`.
#[tauri::command]
pub async fn generate_letter(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    letter_type: Option<String>,
) -> Result<String, String> {
    let _ = app.emit(
        "generation-progress",
        GenerationProgress {
            doc_type: "letter".into(),
            status: "started".into(),
            recording_id: recording_id.clone(),
        },
    );

    let result =
        generate_letter_inner(&state, &recording_id, letter_type.as_deref()).await;

    match &result {
        Ok(_) => {
            let _ = app.emit(
                "generation-progress",
                GenerationProgress {
                    doc_type: "letter".into(),
                    status: "completed".into(),
                    recording_id: recording_id.clone(),
                },
            );
        }
        Err(msg) => {
            let _ = app.emit(
                "generation-progress",
                GenerationProgress {
                    doc_type: "letter".into(),
                    status: format!("failed: {msg}"),
                    recording_id: recording_id.clone(),
                },
            );
        }
    }

    result
}

async fn generate_letter_inner(
    state: &AppState,
    recording_id: &str,
    letter_type: Option<&str>,
) -> Result<String, String> {
    let (provider, mut recording) = get_provider_and_recording(state, recording_id).await?;

    let soap_note = recording
        .soap_note
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Recording has no SOAP note. Generate a SOAP note first.")?;

    let ltype = letter_type.unwrap_or("follow-up");

    let (system_prompt, user_prompt) =
        document_generator::build_letter_prompt(soap_note, ltype);

    debug!(
        "generate_letter: provider='{}', recording='{}', letter_type='{}'",
        provider.name(),
        recording_id,
        ltype,
    );

    let (model, temperature) = load_ai_settings(state);
    let request = build_completion_request(system_prompt, user_prompt, model, temperature);

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

    let letter_text = response.content;
    if letter_text.is_empty() {
        return Err("AI returned an empty letter.".into());
    }

    // Persist to DB
    recording.letter = Some(letter_text.clone());
    {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())?;
    }

    Ok(letter_text)
}

/// Generate a brief synopsis from a recording's SOAP note.
///
/// The synopsis is returned directly and stored in the recording's metadata
/// (the `Recording` struct does not have a dedicated `synopsis` field).
#[tauri::command]
pub async fn generate_synopsis(
    state: tauri::State<'_, AppState>,
    recording_id: String,
) -> Result<String, String> {
    let (provider, mut recording) =
        get_provider_and_recording(&state, &recording_id).await?;

    let soap_note = recording
        .soap_note
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Recording has no SOAP note. Generate a SOAP note first.")?;

    let (system_prompt, user_prompt) = document_generator::build_synopsis_prompt(soap_note);

    debug!(
        "generate_synopsis: provider='{}', recording='{}'",
        provider.name(),
        recording_id,
    );

    let (model, temperature) = load_ai_settings(&*state);
    let request = build_completion_request(system_prompt, user_prompt, model, temperature);

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

    let synopsis_text = response.content;
    if synopsis_text.is_empty() {
        return Err("AI returned an empty synopsis.".into());
    }

    // Store synopsis in the metadata JSON object.
    if recording.metadata.is_null() {
        recording.metadata = serde_json::json!({});
    }
    if let Some(obj) = recording.metadata.as_object_mut() {
        obj.insert(
            "synopsis".to_string(),
            serde_json::Value::String(synopsis_text.clone()),
        );
    }
    {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())?;
    }

    Ok(synopsis_text)
}
