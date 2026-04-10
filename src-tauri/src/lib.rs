mod state;
mod commands;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::initialize()
        .expect("Failed to initialize application state");

    tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::recordings::list_recordings,
            commands::recordings::get_recording,
            commands::recordings::search_recordings,
            commands::recordings::delete_recording,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_api_key,
            commands::settings::set_api_key,
            commands::settings::list_api_keys,
            commands::export::export_pdf,
            commands::export::export_docx,
            commands::export::export_fhir,
            commands::providers::reinit_providers,
            commands::audio::list_audio_devices,
            commands::audio::start_recording,
            commands::audio::stop_recording,
            commands::audio::pause_recording,
            commands::audio::resume_recording,
            commands::chat::chat_send,
            commands::chat::chat_stream,
            commands::chat::chat_with_agent,
            commands::chat::list_ai_providers,
            commands::chat::set_active_provider,
            commands::chat::list_models,
            commands::transcription::transcribe_recording,
            commands::transcription::list_stt_providers,
            commands::generation::generate_soap,
            commands::generation::generate_referral,
            commands::generation::generate_letter,
            commands::generation::generate_synopsis,
            commands::rag::ingest_document,
            commands::rag::search_rag,
            commands::rag::rag_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
