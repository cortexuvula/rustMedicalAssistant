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

/// Transcribe a previously recorded WAV file using the STT failover chain.
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

    // Parse and validate the recording ID.
    let uuid = Uuid::parse_str(&recording_id).map_err(|e| e.to_string())?;

    // Load the recording from the database to ensure it exists.
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    let mut recording =
        RecordingsRepo::get_by_id(&conn, &uuid).map_err(|e| e.to_string())?;

    // Mark the recording as Processing.
    recording.status = ProcessingStatus::Processing {
        started_at: Utc::now(),
    };
    RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())?;

    // Use the audio_path stored in the recording (supports any filename format).
    let wav_path = recording.audio_path.clone();

    if !wav_path.exists() {
        let err_msg = format!("WAV file not found: {}", wav_path.display());
        recording.status = ProcessingStatus::Failed {
            error: err_msg.clone(),
            retry_count: 0,
        };
        let _ = RecordingsRepo::update(&conn, &recording);
        return Err(err_msg);
    }

    let audio = load_wav_to_audio_data(&wav_path)?;

    // Build STT config from caller parameters.
    // Default diarize to true — medical recordings are typically conversations.
    let config = SttConfig {
        language,
        diarize: diarize.unwrap_or(true),
        ..SttConfig::default()
    };

    // --- emit: transcribing ---
    let _ = app.emit("transcription-progress", "transcribing");

    // Clone the Arc<SttFailover> so we release the mutex before the long-running transcribe await.
    let stt = {
        let guard = state.stt_providers.lock().await;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "No STT providers configured. Add an API key in Settings.".to_string())?
    };
    let transcript = stt.transcribe(audio, config)
        .await
        .map_err(|e| e.to_string())?;

    // Build speaker-attributed text when diarization segments are available.
    let display_text = format_transcript_with_speakers(&transcript);

    // Persist the transcript and mark as Completed.
    recording.transcript = Some(display_text.clone());
    recording.stt_provider = Some(transcript.provider.clone());
    recording.status = ProcessingStatus::Completed {
        completed_at: Utc::now(),
    };
    RecordingsRepo::update(&conn, &recording).map_err(|e| e.to_string())?;

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

    // Map raw speaker IDs (speaker_0, speaker_1) to friendly labels (Speaker 1, Speaker 2).
    let mut speaker_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut speaker_counter = 0u32;

    for seg in &segments_with_speakers {
        if let Some(ref id) = seg.speaker {
            if !speaker_map.contains_key(id) {
                speaker_counter += 1;
                speaker_map.insert(id.clone(), format!("Speaker {speaker_counter}"));
            }
        }
    }

    // Group consecutive words by speaker into paragraphs.
    let mut result = String::new();
    let mut current_speaker: Option<&str> = None;
    let mut current_words: Vec<&str> = Vec::new();

    for seg in &segments_with_speakers {
        let speaker_id = seg.speaker.as_deref().unwrap_or("unknown");

        if current_speaker != Some(speaker_id) {
            // Flush the previous speaker's words.
            if !current_words.is_empty() {
                if let Some(prev_id) = current_speaker {
                    let label = speaker_map.get(prev_id).map(|s| s.as_str()).unwrap_or(prev_id);
                    if !result.is_empty() {
                        result.push_str("\n\n");
                    }
                    result.push_str(label);
                    result.push_str(": ");
                    result.push_str(&current_words.join(" "));
                }
                current_words.clear();
            }
            current_speaker = Some(speaker_id);
        }

        current_words.push(seg.text.trim());
    }

    // Flush the last speaker's words.
    if !current_words.is_empty() {
        if let Some(prev_id) = current_speaker {
            let label = speaker_map.get(prev_id).map(|s| s.as_str()).unwrap_or(prev_id);
            if !result.is_empty() {
                result.push_str("\n\n");
            }
            result.push_str(label);
            result.push_str(": ");
            result.push_str(&current_words.join(" "));
        }
    }

    result
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. list_stt_providers
// ──────────────────────────────────────────────────────────────────────────────

/// Return the name and availability status of each configured STT provider.
#[tauri::command]
pub async fn list_stt_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, bool)>, String> {
    let guard = state.stt_providers.lock().await;
    match guard.as_deref() {
        Some(stt) => Ok(stt.provider_statuses()),
        None => Ok(vec![]),
    }
}
