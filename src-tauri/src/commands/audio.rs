use std::time::Instant;

use tauri::Emitter;
use tracing::{info, warn, instrument};
use uuid::Uuid;

use medical_audio::capture::CaptureConfig;
use medical_audio::device::{get_input_device, list_input_devices, AudioDevice};
use medical_core::types::recording::{ProcessingStatus, Recording};
use medical_db::recordings::RecordingsRepo;

use crate::state::{AppState, CurrentRecording, SendCaptureHandle};
use super::resolve_recordings_dir;

// ──────────────────────────────────────────────────────────────────────────────
// 1. list_audio_devices
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    list_input_devices().map_err(|e| e.to_string())
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. start_recording
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
#[instrument(skip(app, state), name = "audio::start_recording")]
pub async fn start_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    info!("Starting audio recording");

    // Ensure we are not already recording.
    {
        let active = state.recording_active.lock().await;
        if *active {
            warn!("Attempted to start recording while another is in progress");
            return Err("A recording is already in progress".into());
        }
    }

    // Resolve recordings directory from settings (custom path or default).
    let recordings_dir = resolve_recordings_dir(&state.db, &state.data_dir)?;

    // Generate UUID and human-readable filename.
    let recording_id = Uuid::new_v4();
    let now = chrono::Local::now();
    let friendly_name = now.format("Recording_%Y-%m-%d_%H-%M-%S").to_string();
    let wav_path = recordings_dir.join(format!("{}.wav", friendly_name));

    // Read the configured input device and sample rate from settings.
    let (input_device_name, sample_rate) = {
        let conn = state.db.conn().map_err(|e| e.to_string())?;
        let config = medical_db::settings::SettingsRepo::load_config(&conn)
            .map_err(|e| e.to_string())?;
        (
            config.input_device.filter(|s| !s.is_empty()),
            config.sample_rate,
        )
    };

    // Capture values for logging before they move into closures.
    let device_name_for_log = input_device_name.clone().unwrap_or_else(|| "default".to_string());
    let wav_path_for_log = wav_path.display().to_string();

    // Start capture on a dedicated std::thread so the !Send CaptureHandle
    // never crosses a thread boundary via tokio::spawn_blocking.  We wrap
    // it in SendCaptureHandle (which has an unsafe Send impl) and send it
    // back through a oneshot channel.
    let wav_path_clone = wav_path.clone();
    let (tx, rx) = std::sync::mpsc::channel::<
        Result<(SendCaptureHandle, std::sync::mpsc::Receiver<Vec<f32>>), String>,
    >();

    std::thread::spawn(move || {
        let result = (|| {
            let device = get_input_device(input_device_name.as_deref())
                .map_err(|e| e.to_string())?;
            let config = CaptureConfig {
                sample_rate,
                ..CaptureConfig::default()
            };
            let (handle, waveform_rx) =
                medical_audio::capture::start_capture(&device, config, &wav_path_clone)
                    .map_err(|e| e.to_string())?;
            Ok((SendCaptureHandle(Some(handle)), waveform_rx))
        })();
        let _ = tx.send(result);
    });

    let (send_handle, waveform_rx) = rx
        .recv()
        .map_err(|_| "Audio capture thread panicked".to_string())??;

    // Store capture handle in AppState.
    {
        let mut handle_lock = state.capture_handle.lock().unwrap();
        *handle_lock = send_handle;
    }

    // Store current recording info.
    {
        let mut rec_lock = state.current_recording.lock().unwrap();
        *rec_lock = Some(CurrentRecording {
            id: recording_id.to_string(),
            wav_path,
            started_at: Instant::now(),
        });
    }

    // Set recording active.
    {
        let mut active = state.recording_active.lock().await;
        *active = true;
    }

    info!(
        recording_id = %recording_id,
        wav_path = %wav_path_for_log,
        device = %device_name_for_log,
        sample_rate,
        "Audio recording started"
    );

    // Spawn a blocking task to consume waveform data and emit Tauri events.
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        while let Ok(data) = waveform_rx.recv() {
            let _ = app_clone.emit("waveform-data", &data);
        }
    });

    Ok(recording_id.to_string())
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. stop_recording
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
#[instrument(skip(state), name = "audio::stop_recording")]
pub async fn stop_recording(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // Take the CaptureHandle out of AppState as a SendCaptureHandle (which is
    // Send+Sync).  We must NOT hold a bare CaptureHandle across an .await
    // because CaptureHandle is !Send.
    let wrapper = {
        let mut handle_lock = state.capture_handle.lock().unwrap();
        let inner = handle_lock.0.take();
        SendCaptureHandle(inner)
    };

    if wrapper.0.is_none() {
        return Err("No active recording to stop".into());
    }

    // Drop the wrapper on a dedicated thread so CaptureHandle::drop (which
    // joins the drain thread) doesn't block the async runtime.
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        drop(wrapper);
        let _ = tx.send(());
    });
    rx.recv().map_err(|_| "Stop thread panicked".to_string())?;

    // Set recording inactive.
    {
        let mut active = state.recording_active.lock().await;
        *active = false;
    }

    // Take the current recording info.
    let current = {
        let mut rec_lock = state.current_recording.lock().unwrap();
        rec_lock.take()
    };

    let current = current.ok_or("No current recording info found")?;

    // Compute duration from elapsed time.
    let duration_secs = current.started_at.elapsed().as_secs_f64();

    // Get file size of the WAV file.
    let file_size = match std::fs::metadata(&current.wav_path) {
        Ok(m) => m.len(),
        Err(e) => {
            tracing::warn!(path = %current.wav_path.display(), error = %e, "Could not read WAV file metadata");
            0
        }
    };
    if file_size == 0 {
        tracing::warn!(path = %current.wav_path.display(), "WAV file is empty — audio may not have been captured");
    }

    let recording_uuid = Uuid::parse_str(&current.id).map_err(|e| e.to_string())?;

    // Build the Recording struct.
    let filename = current
        .wav_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{}.wav", current.id));

    let mut recording = Recording::new(filename, current.wav_path.clone());
    // Override the auto-generated id with our known UUID.
    recording.id = recording_uuid;
    recording.duration_seconds = Some(duration_secs);
    recording.file_size_bytes = Some(file_size);
    recording.status = ProcessingStatus::Pending;

    // Insert into DB.
    let conn = state.db.conn().map_err(|e| e.to_string())?;
    RecordingsRepo::insert(&conn, &recording).map_err(|e| e.to_string())?;

    info!(
        recording_id = %current.id,
        duration_secs = %format!("{:.1}", duration_secs),
        file_size_bytes = file_size,
        wav_path = %current.wav_path.display(),
        "Recording stopped and saved"
    );

    Ok(current.id)
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. cancel_recording
// ──────────────────────────────────────────────────────────────────────────────

/// Cancel the current recording, discarding the audio file without saving.
#[tauri::command]
pub async fn cancel_recording(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Take the CaptureHandle out of AppState.
    let wrapper = {
        let mut handle_lock = state.capture_handle.lock().unwrap();
        let inner = handle_lock.0.take();
        SendCaptureHandle(inner)
    };

    if wrapper.0.is_none() {
        return Err("No active recording to cancel".into());
    }

    // Drop the capture handle on a dedicated thread.
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        drop(wrapper);
        let _ = tx.send(());
    });
    rx.recv().map_err(|_| "Cancel thread panicked".to_string())?;

    // Set recording inactive.
    {
        let mut active = state.recording_active.lock().await;
        *active = false;
    }

    // Take the current recording info and delete the WAV file.
    let current = {
        let mut rec_lock = state.current_recording.lock().unwrap();
        rec_lock.take()
    };

    if let Some(current) = current {
        if current.wav_path.exists() {
            let _ = std::fs::remove_file(&current.wav_path);
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. pause_recording
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn pause_recording(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let handle_lock = state.capture_handle.lock().unwrap();
    match &handle_lock.0 {
        Some(handle) => {
            handle.pause();
            Ok(())
        }
        None => Err("No active recording to pause".into()),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. resume_recording
// ──────────────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn resume_recording(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let handle_lock = state.capture_handle.lock().unwrap();
    match &handle_lock.0 {
        Some(handle) => {
            handle.resume();
            Ok(())
        }
        None => Err("No active recording to resume".into()),
    }
}
