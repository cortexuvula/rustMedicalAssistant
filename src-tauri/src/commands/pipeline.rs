//! Background pipeline: transcribe → generate SOAP in one command.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::Emitter;
use tracing::{info, error, instrument, warn};
use uuid::Uuid;

use medical_core::error::{AppError, AppResult};
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
#[instrument(skip(app, state, context), fields(recording_id = %recording_id))]
pub async fn process_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    context: Option<String>,
    template: Option<String>,
) -> AppResult<String> {
    info!(
        has_context = context.is_some(),
        template = template.as_deref().unwrap_or("default"),
        "Pipeline started: transcribe → SOAP"
    );
    let rid = recording_id.clone();

    // Register a cancel flag so the frontend can ask us to bail between stages.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.pipeline_cancels.lock().unwrap();
        guard.insert(recording_id.clone(), Arc::clone(&cancel));
    }
    // Ensure the flag is removed on every exit path.
    let _cancel_guard = CancelGuard {
        cancels: Arc::clone(&state.pipeline_cancels),
        key: recording_id.clone(),
    };

    // If no explicit template, read the user's preferred template from settings.
    let template = match template {
        Some(t) => Some(t),
        None => {
            let db = std::sync::Arc::clone(&state.db);
            tokio::task::spawn_blocking(move || {
                let conn = db.conn().ok()?;
                let mut cfg = medical_db::settings::SettingsRepo::load_config(&conn).ok()?;
                cfg.migrate();
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

    // Forward the pipeline cancel flag through to the transcription inner
    // helper so the user's cancel click can interrupt transcription at the
    // STT-call boundary instead of waiting for the whole (often 30s+) pass
    // to finish.
    let transcript_result = super::transcription::transcribe_recording_inner(
        app.clone(),
        state.clone(),
        recording_id.clone(),
        None,       // language — use default
        Some(true), // diarize — medical encounters are multi-speaker
        Some(&cancel),
    )
    .await;

    if let Err(e) = transcript_result {
        let msg = e.to_string();
        error!(error = %msg, "Pipeline failed at transcription stage");
        emit_progress(&app, &rid, "failed", Some(msg));
        // Return the original typed error so the frontend receives a
        // structured {kind, message} payload instead of a plain string.
        return Err(e);
    }

    info!("Pipeline stage 1 complete: transcription succeeded");

    if cancel.load(Ordering::SeqCst) {
        let msg = "Pipeline cancelled by user after transcription".to_string();
        warn!("{msg}");
        emit_progress(&app, &rid, "failed", Some(msg.clone()));
        return Err(AppError::Cancelled);
    }

    // --- Stage 2: Generate SOAP ---
    emit_progress(&app, &rid, "generating_soap", None);

    let soap_result = super::generation::generate_soap(
        app.clone(),
        state.clone(),
        recording_id.clone(),
        template,
        context,
        None,
    )
    .await;

    match soap_result {
        Ok(soap_text) => {
            // Fetch the recording name for the notification
            let display_name = get_recording_display_name(&state, &recording_id).await;
            emit_progress(&app, &rid, "completed", None);

            info!(
                soap_len = soap_text.len(),
                "Pipeline complete: transcription + SOAP generation succeeded"
            );

            // Emit a dedicated notification event for the toast
            let _ = app.emit("pipeline-complete", serde_json::json!({
                "recording_id": rid,
                "display_name": display_name,
            }));

            Ok(soap_text)
        }
        Err(e) => {
            let msg = e.to_string();
            error!(error = %msg, "Pipeline failed at SOAP generation stage");
            emit_progress(&app, &rid, "failed", Some(msg));
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

/// RAII helper: remove the cancel flag from the map when the pipeline exits
/// (success, error, or panic unwind) so we don't leak entries.
struct CancelGuard {
    cancels: Arc<std::sync::Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>>,
    key: String,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.cancels.lock() {
            guard.remove(&self.key);
        }
    }
}

/// Signal a running pipeline to cancel at its next stage boundary.
///
/// Returns `true` if a pipeline with that id was found and flagged, `false`
/// if no pipeline was registered under that recording id. Does not kill an
/// in-flight HTTP call — cancellation takes effect between transcription and
/// SOAP generation, or when the current network timeout fires.
#[tauri::command]
pub fn cancel_pipeline(
    state: tauri::State<'_, AppState>,
    recording_id: String,
) -> AppResult<bool> {
    let guard = state
        .pipeline_cancels
        .lock()
        .map_err(|_| AppError::Other("pipeline_cancels mutex poisoned".to_string()))?;
    if let Some(flag) = guard.get(&recording_id) {
        flag.store(true, Ordering::SeqCst);
        info!(recording_id = %recording_id, "Pipeline cancel requested");
        Ok(true)
    } else {
        Ok(false)
    }
}
