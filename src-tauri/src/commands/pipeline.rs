//! Background pipeline: transcribe → generate SOAP in one command.

use std::sync::Arc;

use serde::Serialize;
use tauri::Emitter;
use uuid::Uuid;

use medical_db::recordings::RecordingsRepo;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
struct PipelineProgress {
    recording_id: String,
    stage: String,
    error: Option<String>,
}

/// Run the full transcribe → SOAP pipeline for a recording.
///
/// This command is designed to be called fire-and-forget from the frontend.
/// Progress is reported via `pipeline-progress` events so the frontend can
/// track multiple concurrent pipelines by recording ID.
#[tauri::command]
pub async fn process_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    context: Option<String>,
    template: Option<String>,
) -> Result<String, String> {
    let rid = recording_id.clone();

    // If no explicit template, read the user's preferred template from settings.
    let template = match template {
        Some(t) => Some(t),
        None => {
            let db = std::sync::Arc::clone(&state.db);
            tokio::task::spawn_blocking(move || {
                let conn = db.conn().ok()?;
                let cfg = medical_db::settings::SettingsRepo::load_config(&conn).ok()?;
                let t = match cfg.soap_template {
                    medical_core::types::settings::SoapTemplate::FollowUp => "follow_up",
                    medical_core::types::settings::SoapTemplate::NewPatient => "new_patient",
                    medical_core::types::settings::SoapTemplate::Telehealth => "telehealth",
                    medical_core::types::settings::SoapTemplate::Emergency => "emergency",
                    medical_core::types::settings::SoapTemplate::Pediatric => "pediatric",
                    medical_core::types::settings::SoapTemplate::Geriatric => "geriatric",
                };
                Some(t.to_string())
            })
            .await
            .ok()
            .flatten()
        }
    };

    // --- Stage 1: Transcribe ---
    emit_progress(&app, &rid, "transcribing", None);

    let transcript_result = super::transcription::transcribe_recording(
        app.clone(),
        state.clone(),
        recording_id.clone(),
        None,       // language — use default
        Some(true), // diarize — medical encounters are multi-speaker
    )
    .await;

    if let Err(ref e) = transcript_result {
        emit_progress(&app, &rid, "failed", Some(e.clone()));
        return Err(e.clone());
    }

    // --- Stage 2: Generate SOAP ---
    emit_progress(&app, &rid, "generating_soap", None);

    let soap_result = super::generation::generate_soap(
        app.clone(),
        state.clone(),
        recording_id.clone(),
        template,
        context,
    )
    .await;

    match soap_result {
        Ok(soap_text) => {
            // Fetch the recording name for the notification
            let display_name = get_recording_display_name(&state, &recording_id).await;
            emit_progress(&app, &rid, "completed", None);

            // Emit a dedicated notification event for the toast
            let _ = app.emit("pipeline-complete", serde_json::json!({
                "recording_id": rid,
                "display_name": display_name,
            }));

            Ok(soap_text)
        }
        Err(e) => {
            emit_progress(&app, &rid, "failed", Some(e.clone()));
            Err(e)
        }
    }
}

fn emit_progress(app: &tauri::AppHandle, recording_id: &str, stage: &str, error: Option<String>) {
    let _ = app.emit(
        "pipeline-progress",
        PipelineProgress {
            recording_id: recording_id.to_string(),
            stage: stage.to_string(),
            error,
        },
    );
}

async fn get_recording_display_name(state: &AppState, recording_id: &str) -> String {
    let uuid = match Uuid::parse_str(recording_id) {
        Ok(u) => u,
        Err(_) => return "Recording".to_string(),
    };
    let db = Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().ok()?;
        let rec = RecordingsRepo::get_by_id(&conn, &uuid).ok()?;
        Some(rec.patient_name.unwrap_or(rec.filename))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "Recording".to_string())
}
