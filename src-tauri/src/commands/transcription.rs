use std::sync::Arc;

use chrono::Utc;
use tauri::Emitter;
use uuid::Uuid;

use medical_core::types::recording::ProcessingStatus;
use medical_core::types::stt::{AudioData, SttConfig};
use medical_db::recordings::RecordingsRepo;

use crate::state::AppState;

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Load a WAV file from disk and convert it into `AudioData` (f32 PCM).
fn load_wav_to_audio_data(path: &std::path::Path) -> Result<AudioData, String> {
    let reader =
        hound::WavReader::open(path).map_err(|e| format!("Failed to open WAV: {e}"))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.into_samples::<f32>().filter_map(|s| s.ok()).collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    Ok(AudioData {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. transcribe_recording
// ──────────────────────────────────────────────────────────────────────────────

/// Transcribe a previously recorded WAV file using the local STT provider.
///
/// Emits `transcription-progress` events ("loading", "transcribing", "complete")
/// so the frontend can display live status.  Returns the transcript text on success.
#[tauri::command]
pub async fn transcribe_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    language: Option<String>,
    diarize: Option<bool>,
) -> Result<String, String> {
    // --- emit: loading ---
    let _ = app.emit("transcription-progress", "loading");

    // Parse the recording ID.
    let uuid = Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;

    // Load the recording and mark as Processing — on a blocking thread.
    let db = Arc::clone(&state.db);
    let recording = tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        let mut recording =
            RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())?;

        recording.status = ProcessingStatus::Processing {
            started_at: Utc::now(),
        };
        RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())?;
        Ok::<_, String>(recording)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    let wav_path = recording.audio_path.clone();

    if !wav_path.exists() {
        let err_msg = format!("WAV file not found: {}", wav_path.display());
        // Mark failed on a blocking thread
        let db = Arc::clone(&state.db);
        let mut rec = recording;
        rec.status = ProcessingStatus::Failed {
            error: err_msg.clone(),
            retry_count: 0,
        };
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(conn) = db.conn() {
                let _ = RecordingsRepo::update(&conn, &rec);
            }
        })
        .await;
        return Err(err_msg);
    }

    // Load and decode the WAV file on a blocking thread (CPU-intensive for large files).
    let wav_path_clone = wav_path.clone();
    let audio = tokio::task::spawn_blocking(move || load_wav_to_audio_data(&wav_path_clone))
        .await
        .map_err(|e| format!("Task join error: {e}"))??;

    // Compute audio signal stats to detect silent/corrupt recordings.
    let (peak, rms) = if audio.samples.is_empty() {
        (0.0f32, 0.0f32)
    } else {
        let peak = audio.samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let sum_sq: f64 = audio.samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
        let rms = (sum_sq / audio.samples.len() as f64).sqrt() as f32;
        (peak, rms)
    };

    tracing::info!(
        samples = audio.samples.len(),
        sample_rate = audio.sample_rate,
        channels = audio.channels,
        duration_secs = %format!("{:.1}", audio.duration_seconds()),
        peak_amplitude = %format!("{:.6}", peak),
        rms_level = %format!("{:.6}", rms),
        "Loaded WAV audio data"
    );

    if audio.samples.is_empty() {
        let err_msg = format!("WAV file contains no audio samples: {}", wav_path.display());
        tracing::error!("{err_msg}");
        return Err(err_msg);
    }

    // Build STT config from caller parameters.
    // Default diarize to true — medical recordings are typically conversations.
    let config = SttConfig {
        language,
        diarize: diarize.unwrap_or(true),
        ..SttConfig::default()
    };

    // --- emit: transcribing ---
    let _ = app.emit("transcription-progress", "transcribing");

    let stt: Arc<dyn medical_core::traits::SttProvider + Send + Sync> = {
        let guard = state.stt_providers.lock().await;
        match guard.as_ref() {
            Some(stt) => stt.clone(),
            None => {
                tracing::error!("No STT provider configured — cannot transcribe");
                return Err(
                    "No STT provider configured. Download a Whisper model in Settings → Audio / STT.".to_string()
                );
            }
        }
    };
    let transcript = stt.transcribe(audio, config)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "STT transcription failed");
            format!("Transcription failed: {e}")
        })?;

    tracing::info!(
        provider = %transcript.provider,
        text_len = transcript.text.len(),
        segments = transcript.segments.len(),
        "Transcription complete"
    );

    // Build speaker-attributed text when diarization segments are available.
    let display_text = format_transcript_with_speakers(&transcript);

    // Persist the transcript and mark as Completed — on a blocking thread.
    let db = Arc::clone(&state.db);
    let mut recording = recording;
    recording.transcript = Some(display_text.clone());
    recording.stt_provider = Some(transcript.provider.clone());
    recording.status = ProcessingStatus::Completed {
        completed_at: Utc::now(),
    };
    tokio::task::spawn_blocking(move || {
        let conn = db.conn().map_err(|e| e.to_string())?;
        RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    // --- emit: complete ---
    let _ = app.emit("transcription-progress", "complete");

    Ok(display_text)
}

/// Format a transcript with speaker labels when diarization data is available.
///
/// Groups consecutive segments by speaker and formats as:
///   Speaker 1: Hello, how are you?
///   Speaker 2: I'm not feeling well.
///
/// Falls back to the raw text when no speaker segments are present.
fn format_transcript_with_speakers(transcript: &medical_core::types::stt::Transcript) -> String {
    let segments_with_speakers: Vec<_> = transcript
        .segments
        .iter()
        .filter(|s| s.speaker.is_some())
        .collect();

    if segments_with_speakers.is_empty() {
        return transcript.text.clone();
    }

    // Group consecutive segments by speaker into paragraphs.
    // Speaker labels arrive pre-formatted from the merge module ("Speaker 1", "Speaker 2").
    let mut result = String::new();
    let mut current_speaker: Option<&str> = None;
    let mut current_words: Vec<&str> = Vec::new();

    for seg in &segments_with_speakers {
        let label = seg.speaker.as_deref().unwrap_or("Unknown");

        if current_speaker != Some(label) {
            // Flush the previous speaker's words.
            if !current_words.is_empty() {
                if let Some(prev) = current_speaker {
                    if !result.is_empty() {
                        result.push_str("\n\n");
                    }
                    result.push_str(prev);
                    result.push_str(": ");
                    result.push_str(&current_words.join(" "));
                }
                current_words.clear();
            }
            current_speaker = Some(label);
        }

        current_words.push(seg.text.trim());
    }

    // Flush the last speaker's words.
    if !current_words.is_empty() {
        if let Some(prev) = current_speaker {
            if !result.is_empty() {
                result.push_str("\n\n");
            }
            result.push_str(prev);
            result.push_str(": ");
            result.push_str(&current_words.join(" "));
        }
    }

    result
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. list_stt_providers
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_stt_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, bool)>, String> {
    let guard = state.stt_providers.lock().await;
    match guard.as_ref() {
        Some(provider) => Ok(vec![(provider.name().to_string(), true)]),
        None => Ok(vec![]),
    }
}
