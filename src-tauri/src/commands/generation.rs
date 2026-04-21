//! Tauri commands for AI-powered document generation (SOAP, referral, letter, synopsis).

use std::sync::Arc;

use serde::Serialize;
use tauri::Emitter;
use tracing::{debug, info, error, instrument};
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

/// Loaded settings needed for generation.
struct GenerationSettings {
    model: String,
    temperature: f32,
    icd_version: String,
    ai_provider: String,
    custom_soap_prompt: Option<String>,
    custom_referral_prompt: Option<String>,
    custom_letter_prompt: Option<String>,
    custom_synopsis_prompt: Option<String>,
}

/// Load a recording and settings from DB on a blocking thread.
///
/// All rusqlite work is offloaded via `spawn_blocking` so we never block the
/// Tokio async runtime.
async fn load_recording_and_settings(
    db: &Arc<medical_db::Database>,
    recording_id: &str,
) -> Result<(Recording, GenerationSettings), String> {
    let uuid =
        Uuid::parse_str(recording_id).map_err(|e| format!("Invalid recording ID: {e}"))?;
    let db = Arc::clone(db);

    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;

        let recording =
            RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())?;

        let config = medical_db::settings::SettingsRepo::load_config(&conn)
            .ok()
            .map(|mut c| { c.migrate(); c });
        let settings = match config {
            Some(cfg) => {
                let icd = match cfg.icd_version {
                    medical_core::types::settings::IcdVersion::Icd9 => "ICD-9".to_string(),
                    medical_core::types::settings::IcdVersion::Icd10 => "ICD-10".to_string(),
                    medical_core::types::settings::IcdVersion::Both => "both".to_string(),
                };
                GenerationSettings {
                    model: cfg.ai_model,
                    temperature: cfg.temperature,
                    icd_version: icd,
                    ai_provider: cfg.ai_provider,
                    custom_soap_prompt: cfg.custom_soap_prompt,
                    custom_referral_prompt: cfg.custom_referral_prompt,
                    custom_letter_prompt: cfg.custom_letter_prompt,
                    custom_synopsis_prompt: cfg.custom_synopsis_prompt,
                }
            }
            None => GenerationSettings {
                model: "gpt-4o".to_string(),
                temperature: 0.2,
                icd_version: "ICD-10".to_string(),
                ai_provider: "openai".to_string(),
                custom_soap_prompt: None,
                custom_referral_prompt: None,
                custom_letter_prompt: None,
                custom_synopsis_prompt: None,
            },
        };

        Ok((recording, settings))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

/// Resolve the AI provider from the registry using the settings provider name.
async fn resolve_provider(
    state: &AppState,
    provider_name: &str,
) -> Result<Arc<dyn AiProvider>, String> {
    let registry = state.ai_providers.lock().await;
    registry
        .get_arc(provider_name)
        .or_else(|| registry.get_active_arc())
        .ok_or_else(|| "No AI provider configured. Add an API key in Settings.".to_string())
}

/// Persist a recording update on a blocking thread.
async fn persist_recording(
    db: &Arc<medical_db::Database>,
    recording: Recording,
) -> Result<(), String> {
    let db = Arc::clone(db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
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

/// Build a single-turn `CompletionRequest` from system and user prompts.
fn build_completion_request(
    system_prompt: String,
    user_prompt: String,
    model: String,
    temperature: f32,
    max_tokens: Option<u32>,
) -> CompletionRequest {
    CompletionRequest {
        model,
        messages: vec![Message {
            role: Role::User,
            content: MessageContent::Text(user_prompt),
            tool_calls: vec![],
        }],
        temperature: Some(temperature),
        max_tokens,
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
    context: Option<String>,
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

    let result = generate_soap_inner(&state, &recording_id, template.as_deref(), context.as_deref()).await;

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

#[instrument(skip(state, context), fields(recording_id = %recording_id))]
async fn generate_soap_inner(
    state: &AppState,
    recording_id: &str,
    template: Option<&str>,
    context: Option<&str>,
) -> Result<String, String> {
    let (mut recording, settings) =
        load_recording_and_settings(&state.db, recording_id).await?;
    let provider = resolve_provider(state, &settings.ai_provider).await?;

    let transcript = recording
        .transcript
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or("Recording has no transcript. Run transcription first.")?;

    info!(
        provider = %provider.name(),
        model = %settings.model,
        template = template.unwrap_or("follow_up"),
        transcript_len = transcript.len(),
        context_len = context.map(|c| c.len()).unwrap_or(0),
        "Generating SOAP note"
    );

    // Build prompts with full config
    let soap_template = template.map(parse_soap_template).unwrap_or_default();
    let model_name = settings.model.clone();
    let config = SoapPromptConfig {
        template: soap_template,
        icd_version: settings.icd_version,
        custom_prompt: settings.custom_soap_prompt,
    };

    let system_prompt = soap_generator::build_soap_prompt(&config);
    let user_prompt = soap_generator::build_user_prompt(transcript, context);

    debug!(
        "generate_soap: provider='{}', recording='{}', context_len={}, context_preview='{}'",
        provider.name(),
        recording_id,
        context.map(|c| c.len()).unwrap_or(0),
        context.map(|c| &c[..c.len().min(100)]).unwrap_or("(none)"),
    );
    let request = build_completion_request(
        system_prompt,
        user_prompt,
        settings.model,
        settings.temperature,
        None,
    );

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

    let raw_soap = response.content;
    if raw_soap.is_empty() {
        error!(
            provider = %provider.name(),
            model = %model_name,
            "AI returned an empty SOAP note"
        );
        return Err(format!(
            "AI returned an empty SOAP note (provider: {}, model: {}). \
             Check that the model is loaded and responding.",
            provider.name(),
            model_name,
        ));
    }

    info!(
        raw_len = raw_soap.len(),
        "AI completion received, post-processing"
    );

    // Post-process: strip markdown, fix paragraph formatting
    let soap_text = soap_generator::postprocess_soap(&raw_soap);

    // Save context to recording metadata for future reference
    if let Some(ctx) = context {
        if !ctx.is_empty() {
            if recording.metadata.is_null() {
                recording.metadata = serde_json::json!({});
            }
            if let Some(obj) = recording.metadata.as_object_mut() {
                obj.insert("context".to_string(), serde_json::Value::String(ctx.to_string()));
            }
        }
    }

    // Persist to DB (on blocking thread)
    recording.soap_note = Some(soap_text.clone());
    persist_recording(&state.db, recording).await?;

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
    let (mut recording, settings) =
        load_recording_and_settings(&state.db, recording_id).await?;
    let provider = resolve_provider(state, &settings.ai_provider).await?;

    let soap_note = recording
        .soap_note
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Recording has no SOAP note. Generate a SOAP note first.")?;

    let recipient = recipient_type.unwrap_or("Specialist");
    let urg = urgency.unwrap_or("routine");

    let (system_prompt, user_prompt) =
        document_generator::build_referral_prompt(soap_note, recipient, urg, settings.custom_referral_prompt.as_deref());

    debug!(
        "generate_referral: provider='{}', recording='{}', recipient='{}', urgency='{}'",
        provider.name(),
        recording_id,
        recipient,
        urg,
    );

    let request = build_completion_request(
        system_prompt,
        user_prompt,
        settings.model,
        settings.temperature,
        None,
    );

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

    let referral_text = response.content;
    if referral_text.is_empty() {
        return Err("AI returned an empty referral letter.".into());
    }

    // Persist to DB (on blocking thread)
    recording.referral = Some(referral_text.clone());
    persist_recording(&state.db, recording).await?;

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
    let (mut recording, settings) =
        load_recording_and_settings(&state.db, recording_id).await?;
    let provider = resolve_provider(state, &settings.ai_provider).await?;

    let soap_note = recording
        .soap_note
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Recording has no SOAP note. Generate a SOAP note first.")?;

    let ltype = letter_type.unwrap_or("follow-up");

    let (system_prompt, user_prompt) =
        document_generator::build_letter_prompt(soap_note, ltype, settings.custom_letter_prompt.as_deref());

    debug!(
        "generate_letter: provider='{}', recording='{}', letter_type='{}'",
        provider.name(),
        recording_id,
        ltype,
    );

    let request = build_completion_request(
        system_prompt,
        user_prompt,
        settings.model,
        settings.temperature,
        None,
    );

    let response = provider
        .complete(request)
        .await
        .map_err(|e| format!("AI completion failed: {e}"))?;

    let letter_text = response.content;
    if letter_text.is_empty() {
        return Err("AI returned an empty letter.".into());
    }

    // Persist to DB (on blocking thread)
    recording.letter = Some(letter_text.clone());
    persist_recording(&state.db, recording).await?;

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
    let (mut recording, settings) =
        load_recording_and_settings(&state.db, &recording_id).await?;
    let provider = resolve_provider(&state, &settings.ai_provider).await?;

    let soap_note = recording
        .soap_note
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Recording has no SOAP note. Generate a SOAP note first.")?;

    let (system_prompt, user_prompt) = document_generator::build_synopsis_prompt(soap_note, settings.custom_synopsis_prompt.as_deref());

    debug!(
        "generate_synopsis: provider='{}', recording='{}'",
        provider.name(),
        recording_id,
    );

    let request = build_completion_request(
        system_prompt,
        user_prompt,
        settings.model,
        settings.temperature,
        None,
    );

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
    persist_recording(&state.db, recording).await?;

    Ok(synopsis_text)
}
