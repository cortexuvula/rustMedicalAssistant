//! Screenshot-region → OCR → clipboard (v0.75).
//!
//! The feature mirrors Omarchy's `SUPER+CTRL+PRINT` flow: trigger →
//! drag-select a screen region → OCR through the configured **local** vision
//! model → extracted text lands on the clipboard. Triggers:
//!
//! - the global hotkey (default `CmdOrCtrl+Alt+O`; Wayland users bind the
//!   compositor instead — see Settings),
//! - the `--capture-ocr` CLI flag, delegated to the running instance via
//!   `tauri-plugin-single-instance`,
//! - an in-app button / in-app shortcut.
//!
//! All triggers converge on [`run_capture_ocr`]. Expected non-outcomes
//! (cancelled selection, empty extraction) come back as typed outcomes, not
//! errors — matching the translate capture pattern.

use std::sync::atomic::{AtomicBool, Ordering};

use medical_core::error::{AppError, AppResult};
use medical_core::types::settings::AppConfig;

use crate::state::AppState;

/// Default hotkey: Cmd+Option+O on macOS, Ctrl+Alt+O elsewhere.
pub const DEFAULT_HOTKEY: &str = "CmdOrCtrl+Alt+O";

/// Event channel the frontend toasts on (headless triggers can't return a
/// command result). Payload carries counts only — never extracted text.
pub const OCR_EVENT: &str = "screenshot-ocr";

/// Serializes captures app-wide: one region selection at a time. A second
/// trigger while the picker is open is rejected instead of stacking overlays.
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Outcome of a capture run, returned to the invoking UI.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CaptureOcrOutcome {
    /// `"copied"` — text is on the clipboard; `"cancelled"` — user dismissed
    /// the selection; `"empty"` — the model found no text.
    pub status: &'static str,
    /// Extracted character count (0 unless `copied`).
    pub chars: usize,
}

/// Event payload for headless-triggered runs (global hotkey / CLI delegation).
/// Counts and error text only — no PHI.
#[derive(Debug, Clone, serde::Serialize)]
struct ScreenshotOcrEvent {
    status: String,
    chars: usize,
    error: Option<String>,
}

/// Parse a second-instance argv for the headless capture request.
pub fn wants_capture_ocr(argv: &[String]) -> bool {
    argv.iter().any(|a| a == "--capture-ocr")
}

/// Fire a capture from a non-command context (global hotkey handler,
/// single-instance callback). Spawns detached; results/errors surface as
/// [`OCR_EVENT`] events the frontend toasts on.
pub fn trigger_capture(app: &tauri::AppHandle) {
    use tauri::Manager;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            emit_event(
                &app,
                &ScreenshotOcrEvent {
                    status: "failed".into(),
                    chars: 0,
                    error: Some(
                        "Screenshot OCR is unavailable — the app is in recovery mode.".into(),
                    ),
                },
            );
            return;
        };
        match run_capture_ocr(&app, &state).await {
            Ok(outcome) => {
                emit_event(
                    &app,
                    &ScreenshotOcrEvent {
                        status: outcome.status.to_string(),
                        chars: outcome.chars,
                        error: None,
                    },
                );
            }
            Err(e) => {
                // A second trigger while a capture is running (double hotkey
                // press, hotkey + CLI) is a no-op — don't toast it.
                if e.to_string().contains("already in progress") {
                    tracing::debug!("screenshot OCR trigger ignored: capture already running");
                    return;
                }
                tracing::warn!(error = %e, "screenshot OCR failed");
                emit_event(
                    &app,
                    &ScreenshotOcrEvent {
                        status: "failed".into(),
                        chars: 0,
                        error: Some(e.to_string()),
                    },
                );
            }
        }
    });
}

fn emit_event(app: &tauri::AppHandle, payload: &ScreenshotOcrEvent) {
    use tauri::Emitter;
    if let Err(e) = app.emit(OCR_EVENT, payload) {
        tracing::debug!(error = %e, "screenshot OCR event emit failed");
    }
}

/// Clean a vision model's OCR output before it lands on the clipboard.
///
/// Some OCR-finetuned models (glm-observed with glm-ocr) echo the extracted
/// text a second time inside a ```-fenced block and then pad the tail with
/// dozens of bare ``` lines — unusable as pasted text. When the output
/// contains any fenced block, prefer the text OUTSIDE the fences (the plain
/// extraction); when everything is fenced, take the inside of the first
/// block. Fence-free output passes through untouched, so well-behaved models
/// are unaffected.
///
/// Trade-off: a screenshot OF markdown source (where fences are content)
/// loses its fenced sections. For a quick-capture-to-clipboard tool, clean
/// text is the better default.
fn clean_ocr_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.contains("```") {
        return trimmed.to_string();
    }
    let mut outside: Vec<&str> = Vec::new();
    let mut inside_first: Option<Vec<&str>> = None;
    let mut in_fence = false;
    for line in trimmed.lines() {
        if line.trim_start().starts_with("```") {
            if !in_fence && inside_first.is_none() {
                inside_first = Some(Vec::new());
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            if let Some(lines) = inside_first.as_mut() {
                lines.push(line);
            }
        } else {
            outside.push(line);
        }
    }
    let outside_text = outside.join("\n").trim().to_string();
    if !outside_text.is_empty() {
        return outside_text;
    }
    if let Some(lines) = inside_first {
        let inside = lines.join("\n").trim().to_string();
        if !inside.is_empty() {
            return inside;
        }
    }
    trimmed.to_string()
}

/// Region-capture → OCR → clipboard, callable from the frontend (Settings
/// button, in-app shortcut).
#[tauri::command]
pub async fn capture_region_ocr(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<CaptureOcrOutcome> {
    // The hotkey path (trigger_capture) logs its own failures; log here too
    // so command-path failures (provider down, model errors) are never
    // silent in the app log — the frontend toast alone isn't diagnosable
    // after the fact.
    match run_capture_ocr(&app, &state).await {
        Ok(outcome) => Ok(outcome),
        Err(e) => {
            tracing::warn!(error = %e, "screenshot OCR command failed");
            Err(e)
        }
    }
}

/// The shared flow behind every trigger (command invoke, global hotkey,
/// CLI delegation — the latter two wrap it in [`trigger_capture`], which
/// emits the outcome as an event because they have no caller to return to).
pub async fn run_capture_ocr(
    app: &tauri::AppHandle,
    state: &AppState,
) -> AppResult<CaptureOcrOutcome> {
    if IN_FLIGHT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::InvalidInput(
            "A screenshot OCR capture is already in progress.".into(),
        ));
    }
    let result = capture_ocr_inner(app, state).await;
    IN_FLIGHT.store(false, Ordering::SeqCst);
    result
}

async fn capture_ocr_inner(
    app: &tauri::AppHandle,
    state: &AppState,
) -> AppResult<CaptureOcrOutcome> {
    // Resolve config/model BEFORE showing any picker: a missing model must
    // fail fast with an actionable message, not after the user selects.
    let config = crate::commands::load_app_config(&state.db, "screenshot OCR").await?;
    let ocr_model =
        crate::commands::feature_model_or_global(config.ocr_model.as_deref(), &config.ai_model);
    if ocr_model.is_empty() {
        return Err(AppError::InvalidInput(
            "No OCR model configured. Set an OCR model (or a default AI model) in Settings → Models.".into(),
        ));
    }
    let provider =
        crate::commands::generation::resolve_provider(state, &config.ai_provider).await?;

    // Interactive region selection. Cancel is an expected outcome, not an error.
    let png = match crate::screen_capture::capture_region_png(app, &state.data_dir).await {
        Ok(bytes) => bytes,
        Err(crate::screen_capture::RegionCaptureError::Cancelled) => {
            return Ok(CaptureOcrOutcome {
                status: "cancelled",
                chars: 0,
            });
        }
        Err(e) => return Err(AppError::Other(format!("Screen capture failed: {e}"))),
    };

    // Local vision model only — `ocr_image_bytes` routes through the same
    // provider stack as document OCR. No new network surface.
    let text = medical_processing::ocr::ocr_image_bytes(&png, "png", &ocr_model, &provider)
        .await
        .map_err(|e| AppError::Other(format!("OCR failed: {e}")))?;

    let text = clean_ocr_text(&text);
    if text.is_empty() {
        return Ok(CaptureOcrOutcome {
            status: "empty",
            chars: 0,
        });
    }

    // Write from the Rust side so the flow completes even when the webview
    // isn't focused. Text only — screenshot pixel data NEVER reaches the
    // clipboard (macOS/Windows sync clipboard history to the cloud).
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(&text)
        .map_err(|e| AppError::Other(format!("Clipboard write failed: {e}")))?;

    // Counts only in logs — never extracted content.
    tracing::info!(
        chars = text.len(),
        "screenshot OCR text copied to clipboard"
    );
    Ok(CaptureOcrOutcome {
        status: "copied",
        chars: text.len(),
    })
}

// ---------------------------------------------------------------------------
// Global hotkey registration
// ---------------------------------------------------------------------------

/// The configured hotkey, falling back to [`DEFAULT_HOTKEY`] for unset or
/// blank values.
pub fn resolve_hotkey(config: &AppConfig) -> &str {
    config
        .screenshot_ocr_hotkey
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_HOTKEY)
}

/// Validate a custom hotkey string at save time so Settings gets immediate
/// feedback instead of a silently-dead binding at next launch.
pub fn validate_hotkey(config: &AppConfig) -> AppResult<()> {
    let custom = match config
        .screenshot_ocr_hotkey
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(c) => c,
        None => return Ok(()),
    };
    parse_shortcut(custom).map(|_| ()).map_err(|e| {
        AppError::InvalidInput(format!("Invalid screenshot OCR shortcut '{custom}': {e}"))
    })
}

fn parse_shortcut(s: &str) -> Result<(), String> {
    use std::str::FromStr;
    tauri_plugin_global_shortcut::Shortcut::from_str(s)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// (Re)register the screenshot-OCR hotkey to match `config`. Called at boot
/// and after every settings save. Idempotent: unregisters everything first.
///
/// Registration failure is degraded, never fatal: the binding may conflict
/// with another app, and under Wayland the plugin cannot register at all
/// (X11-only) — those users get the in-app trigger plus the documented
/// compositor binding instead.
pub fn sync_hotkey_registration(app: &tauri::AppHandle, config: &AppConfig) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    if !config.screenshot_ocr_hotkey_enabled {
        return;
    }
    let hotkey = resolve_hotkey(config);
    match shortcuts.register(hotkey) {
        Ok(()) => tracing::info!(hotkey, "screenshot OCR hotkey registered"),
        Err(e) => tracing::warn!(
            error = %e,
            hotkey,
            "screenshot OCR hotkey registration failed (conflict, or Wayland where compositors own hotkeys — use the in-app trigger or a compositor binding)"
        ),
    }
}

// ---------------------------------------------------------------------------
// Desktop notification (cold-start rule)
// ---------------------------------------------------------------------------

/// Fire-and-forget desktop notification. Used by the `--capture-ocr`
/// cold-start path, where there is no app UI to toast in — stdout is
/// invisible under `windows_subsystem = "windows"`, so a notification is the
/// only user-visible surface. Best-effort: failures are ignored.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub fn notify_desktop(title: &str, body: &str) {
    if let Err(e) = spawn_notification(title, body) {
        tracing::debug!(error = %e, "desktop notification spawn failed");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn notify_desktop(_title: &str, _body: &str) {}

#[cfg(target_os = "macos")]
fn spawn_notification(title: &str, body: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display notification \"{}\" with title \"{}\"",
            applescript_escape(body),
            applescript_escape(title)
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}

#[cfg(target_os = "linux")]
fn spawn_notification(title: &str, body: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("notify-send")
        .arg(title)
        .arg(body)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}

#[cfg(target_os = "windows")]
fn spawn_notification(title: &str, body: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command"])
        .arg(format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType=WindowsRuntime] > $null; \
             $t=[Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
             $t.GetElementsByTagName('text').Item(0).AppendChild($t.CreateTextNode('{}')) > $null; \
             $t.GetElementsByTagName('text').Item(1).AppendChild($t.CreateTextNode('{}')) > $null; \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{}').Show([Windows.UI.Notifications.ToastNotification]::new($t))",
            ps_escape(title),
            ps_escape(body),
            env!("CARGO_PKG_NAME")
        ))
        .spawn()
}

#[cfg(target_os = "macos")]
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "windows")]
fn ps_escape(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_capture_flag_in_second_instance_argv() {
        assert!(wants_capture_ocr(&[
            "/usr/bin/rust-medical-assistant".into(),
            "--capture-ocr".into()
        ]));
        assert!(!wants_capture_ocr(&[
            "/usr/bin/rust-medical-assistant".into()
        ]));
        assert!(!wants_capture_ocr(&[]));
    }

    #[test]
    fn hotkey_resolves_default_when_unset_or_blank() {
        let mut config = AppConfig::default();
        assert_eq!(resolve_hotkey(&config), DEFAULT_HOTKEY);
        config.screenshot_ocr_hotkey = Some("   ".into());
        assert_eq!(resolve_hotkey(&config), DEFAULT_HOTKEY);
        config.screenshot_ocr_hotkey = Some("Ctrl+Shift+O".into());
        assert_eq!(resolve_hotkey(&config), "Ctrl+Shift+O");
    }

    #[test]
    fn hotkey_validation_accepts_known_good_and_rejects_garbage() {
        let mut config = AppConfig::default();
        // Unset → default binding is used, no error.
        assert!(validate_hotkey(&config).is_ok());
        config.screenshot_ocr_hotkey = Some(DEFAULT_HOTKEY.into());
        assert!(validate_hotkey(&config).is_ok());
        config.screenshot_ocr_hotkey = Some("CmdOrCtrl+Alt+O".into());
        assert!(validate_hotkey(&config).is_ok());
        config.screenshot_ocr_hotkey = Some("not a shortcut".into());
        assert!(validate_hotkey(&config).is_err());
        config.screenshot_ocr_hotkey = Some("".into());
        assert!(validate_hotkey(&config).is_ok(), "empty means default");
    }

    #[test]
    fn outcome_serializes_status_and_count() {
        let json = serde_json::to_string(&CaptureOcrOutcome {
            status: "copied",
            chars: 42,
        })
        .unwrap();
        assert!(json.contains("\"status\":\"copied\""));
        assert!(json.contains("\"chars\":42"));
    }

    #[test]
    fn clean_ocr_text_passes_fence_free_output_through() {
        assert_eq!(clean_ocr_text("  HbA1c 7.2 %  "), "HbA1c 7.2 %");
        assert_eq!(clean_ocr_text("line one\nline two\n"), "line one\nline two");
        assert_eq!(clean_ocr_text("   "), "");
    }

    #[test]
    fn clean_ocr_text_strips_glm_ocr_echo_and_fence_tail() {
        // The exact shape glm-ocr produced in the live dry-run (2026-09-06):
        // clean extraction, then a fenced duplicate, then dozens of bare
        // fences (elided here to a few).
        let raw = "HbA1c 7.2 %\n\nNext review: 3 months\n```markdown\n\nHbA1c 7.2 %\n\nNext review: 3 months\n```\n```\n```\n```\n       \n```";
        assert_eq!(clean_ocr_text(raw), "HbA1c 7.2 %\n\nNext review: 3 months");
    }

    #[test]
    fn clean_ocr_text_unwraps_fully_fenced_output() {
        let raw = "```markdown\nHbA1c 7.2 %\n```\n```";
        assert_eq!(clean_ocr_text(raw), "HbA1c 7.2 %");
    }

    #[test]
    fn clean_ocr_text_falls_back_to_raw_when_fences_hold_nothing() {
        // Degenerate: fences but no text anywhere usable.
        assert_eq!(clean_ocr_text("```\n\n```\n```"), "```\n\n```\n```".trim());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn applescript_escape_quotes_and_backslashes() {
        assert_eq!(applescript_escape("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(applescript_escape("back\\slash"), "back\\\\slash");
    }
}
