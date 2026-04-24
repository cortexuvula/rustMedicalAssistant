use std::sync::Arc;

use chrono::Utc;
use tauri::Emitter;
use tracing::{info, instrument};
use uuid::Uuid;

use medical_core::types::recording::ProcessingStatus;
use medical_core::types::stt::{AudioData, SttConfig};
use medical_db::recordings::RecordingsRepo;
use medical_db::vocabulary::VocabularyRepo;
use medical_db::settings::SettingsRepo;
use medical_processing::vocabulary_corrector;

use crate::state::AppState;

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Detect the repeated-short-phrase pattern Whisper produces when fed silence
/// (classic: "Thank you. Thank you. Thank you. ...").
///
/// Conservative by design: requires at least 3 sentence-like segments that are
/// all identical (case-insensitive, whitespace-normalised) and short. Callers
/// should gate this on a known-silent source so legitimate short transcripts
/// aren't rejected.
fn is_repeated_phrase_hallucination(text: &str) -> bool {
    let segments: Vec<String> = text
        .split(|c: char| matches!(c, '.' | '!' | '?' | '\n'))
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if segments.len() < 3 {
        return false;
    }
    let first = &segments[0];
    // Long segments are almost never hallucinations — real speech is varied.
    if first.chars().count() > 80 {
        return false;
    }
    segments.iter().all(|s| s == first)
}

/// Load a WAV file from disk and convert it into `AudioData` (f32 PCM).
fn load_wav_to_audio_data(path: &std::path::Path) -> Result<AudioData, String> {
    let reader =
        hound::WavReader::open(path).map_err(|e| format!("Failed to open WAV: {e}"))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader
                .into_samples::<f32>()
                .collect::<Result<Vec<f32>, _>>()
                .map_err(|e| format!("Corrupt WAV sample data: {e}"))?
        }
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .collect::<Result<Vec<i32>, _>>()
                .map_err(|e| format!("Corrupt WAV sample data: {e}"))?
                .into_iter()
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

/// Persist `Failed` status for a recording. Ignores DB errors — the caller is
/// already returning the original error, so a DB write failure here would only
/// obscure it. This is the testable half of `mark_recording_failed`.
pub(super) async fn mark_recording_failed_db_only(
    db: &Arc<medical_db::Database>,
    mut recording: medical_core::types::recording::Recording,
    err_msg: String,
) {
    recording.status = ProcessingStatus::Failed {
        error: err_msg,
        retry_count: 0,
    };
    let db = Arc::clone(db);
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(conn) = db.conn() {
            let _ = RecordingsRepo::update(&conn, &recording);
        }
    })
    .await;
}

/// Mark a recording as `Failed`, persist the status, and emit
/// `transcription-progress: "failed"` so the frontend spinner clears.
///
/// Returns the error message unchanged so callers can
/// `return Err(mark_recording_failed(...).await);`.
async fn mark_recording_failed(
    app: &tauri::AppHandle,
    db: &Arc<medical_db::Database>,
    recording: medical_core::types::recording::Recording,
    err_msg: String,
) -> String {
    mark_recording_failed_db_only(db, recording, err_msg.clone()).await;
    let _ = app.emit("transcription-progress", "failed");
    err_msg
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. transcribe_recording
// ──────────────────────────────────────────────────────────────────────────────

/// Transcribe a previously recorded WAV file using the local STT provider.
///
/// Emits `transcription-progress` events ("loading", "transcribing", "complete")
/// so the frontend can display live status.  Returns the transcript text on success.
#[tauri::command]
#[instrument(skip(app, state), fields(recording_id = %recording_id))]
pub async fn transcribe_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    language: Option<String>,
    diarize: Option<bool>,
) -> Result<String, String> {
    info!(
        language = language.as_deref().unwrap_or("auto"),
        diarize = diarize.unwrap_or(true),
        "Transcription requested"
    );

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
        return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
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

    // Detect near-silent recordings: RMS below -60 dBFS (~0.001) means the
    // microphone likely captured no speech.  Warn but proceed — Whisper's
    // empty-segment filter will catch it if there truly is no speech.
    if !audio.samples.is_empty() && rms < 0.001 {
        tracing::warn!(
            peak = %format!("{:.6}", peak),
            rms = %format!("{:.6}", rms),
            "Recording appears to be silent or near-silent — transcription may produce no text"
        );
    }

    if audio.samples.is_empty() {
        let err_msg = format!("WAV file contains no audio samples: {}", wav_path.display());
        tracing::error!("{err_msg}");
        return Err(mark_recording_failed(&app, &state.db, recording.clone(), err_msg).await);
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
                let err_msg = "No STT provider configured. Download a Whisper model in Settings → Audio / STT.".to_string();
                tracing::error!("{err_msg}");
                return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
            }
        }
    };
    let transcript = match stt.transcribe(audio, config).await {
        Ok(t) => t,
        Err(e) => {
            let err_msg = format!("Transcription failed: {e}");
            tracing::error!(error = %e, "STT transcription failed");
            return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
        }
    };

    tracing::info!(
        provider = %transcript.provider,
        text_len = transcript.text.len(),
        segments = transcript.segments.len(),
        "Transcription complete"
    );

    // Build speaker-attributed text when diarization segments are available.
    let display_text = format_transcript_with_speakers(&transcript);

    // Guard: silent source + repeated-phrase output is a Whisper hallucination.
    // Rejecting here stops us from generating a bogus SOAP from nonsense like
    // "Thank you. Thank you. Thank you. ..." that Whisper emits on silence.
    if rms < 0.001 && is_repeated_phrase_hallucination(&transcript.text) {
        let err_msg = format!(
            "Transcription rejected: the audio was effectively silent (rms={rms:.6}) and the model returned a repeated-phrase hallucination. Check your microphone or audio routing."
        );
        tracing::warn!(
            provider = %transcript.provider,
            rms = %format!("{:.6}", rms),
            text_preview = %transcript.text.chars().take(80).collect::<String>(),
            "Rejecting likely Whisper hallucination from silent source"
        );
        return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
    }

    // Guard: if transcription produced no text, mark as Failed rather than
    // silently storing an empty transcript as "Completed".
    if display_text.trim().is_empty() {
        let err_msg = "Transcription produced no text — the recording may be silent or too short.".to_string();
        tracing::warn!(
            provider = %transcript.provider,
            segments = transcript.segments.len(),
            "{err_msg}"
        );
        return Err(mark_recording_failed(&app, &state.db, recording, err_msg).await);
    }

    // Apply vocabulary corrections if enabled
    let db_vocab = Arc::clone(&state.db);
    let display_text = tokio::task::spawn_blocking(move || {
        let conn = db_vocab.conn().map_err(|e| e.to_string())?;
        let config = SettingsRepo::load_config(&conn)
            .ok()
            .map(|mut c| { c.migrate(); c });
        let vocab_enabled = config.map(|c| c.vocabulary_enabled).unwrap_or(true);
        if vocab_enabled {
            let entries = VocabularyRepo::list_enabled(&conn).map_err(|e| e.to_string())?;
            if !entries.is_empty() {
                let result = vocabulary_corrector::apply_corrections(&display_text, &entries);
                if result.total_replacements > 0 {
                    tracing::info!(
                        replacements = result.total_replacements,
                        "Applied vocabulary corrections to transcript"
                    );
                }
                return Ok::<String, String>(result.corrected_text);
            }
        }
        Ok(display_text)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

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

#[cfg(test)]
mod tests {
    use super::is_repeated_phrase_hallucination;

    use chrono::Utc;
    use medical_core::types::recording::{ProcessingStatus, Recording};
    use medical_db::recordings::RecordingsRepo;
    use medical_db::Database;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn mk_recording() -> Recording {
        let mut rec = Recording::new("t.wav", PathBuf::from("/tmp/nope.wav"));
        rec.status = ProcessingStatus::Processing {
            started_at: Utc::now(),
        };
        rec
    }

    #[tokio::test]
    async fn mark_recording_failed_updates_status_to_failed() {
        let db = Arc::new(Database::open_in_memory().expect("open in-memory db"));
        let rec = mk_recording();
        let id = rec.id;
        {
            let conn = db.conn().expect("conn");
            RecordingsRepo::insert(&conn, &rec).expect("insert");
        }

        super::mark_recording_failed_db_only(&db, rec, "boom".to_string()).await;

        let conn = db.conn().expect("conn");
        let loaded = RecordingsRepo::get_by_id(&conn, &id).expect("get");
        match loaded.status {
            ProcessingStatus::Failed { error, retry_count } => {
                assert_eq!(error, "boom");
                assert_eq!(retry_count, 0);
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn detects_thank_you_hallucination() {
        assert!(is_repeated_phrase_hallucination(
            "Thank you. Thank you. Thank you. Thank you."
        ));
    }

    #[test]
    fn detects_case_insensitive_repetition() {
        assert!(is_repeated_phrase_hallucination(
            "thank you. Thank You. THANK YOU."
        ));
    }

    #[test]
    fn rejects_varied_speech() {
        assert!(!is_repeated_phrase_hallucination(
            "The patient reports fatigue. Blood pressure is 140 over 90. Continue current medications."
        ));
    }

    #[test]
    fn rejects_short_transcript() {
        assert!(!is_repeated_phrase_hallucination("Thank you."));
        assert!(!is_repeated_phrase_hallucination("Thank you. Thank you."));
    }

    #[test]
    fn rejects_empty_transcript() {
        assert!(!is_repeated_phrase_hallucination(""));
        assert!(!is_repeated_phrase_hallucination("   "));
    }

    #[test]
    fn rejects_long_repeated_segments() {
        // Long repeated segments are probably real speech, not hallucination.
        let long = "a".repeat(100);
        let text = format!("{long}. {long}. {long}.");
        assert!(!is_repeated_phrase_hallucination(&text));
    }
}
