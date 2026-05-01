use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::info;

use medical_ai_providers::ollama::OllamaProvider;
use medical_ai_providers::lmstudio::LmStudioProvider;
use medical_ai_providers::http_client::RetryConfig;
use medical_ai_providers::ProviderRegistry;

use medical_core::types::settings::AppConfig;

use medical_agents::orchestrator::AgentOrchestrator;
use medical_agents::tools::{RagSearchTool, ToolRegistry};

use medical_audio::capture::CaptureHandle;

use medical_db::recordings::RecordingsRepo;
use medical_db::Database;

use medical_rag::bm25::Bm25Search;
use medical_rag::embeddings::EmbeddingGenerator;
use medical_rag::graph_search::GraphSearch;
use medical_rag::ingestion::IngestionPipeline;
use medical_rag::vector_store::VectorStore;

use medical_security::key_storage::KeyStorage;

use medical_stt_providers::models as stt_models;

use medical_core::traits::SttProvider;

/// Wrapper to make `CaptureHandle` usable across threads.
///
/// `CaptureHandle` holds a `cpal::Stream`, which cpal marks `!Send` on every
/// platform as a defensive measure — the underlying audio APIs are actually
/// thread-safe on each of the three desktop platforms we target:
///
/// * **macOS (CoreAudio):** `AudioUnit*` APIs are documented as thread-safe
///   since 10.13; audio callbacks already run on a dedicated real-time thread
///   and `AudioOutputUnitStop` / `AudioUnitUninitialize` can be invoked from
///   any thread.
/// * **Windows (WASAPI):** `IAudioClient::Stop` and `Release` can be called
///   from any thread; the stream is stopped before any cross-thread drop.
/// * **Linux (ALSA):** `snd_pcm_drop` / `snd_pcm_close` are documented as safe
///   from any thread so long as the handle is only touched by one caller at a
///   time.
///
/// Our invariants:
/// * All access to `CaptureHandle`'s methods (`pause`/`resume`, `Drop`) is
///   serialised through `AppState::capture_handle` (a `std::sync::Mutex`),
///   so no two threads ever reach the inner FFI simultaneously.
/// * The handle is moved to a `tokio::task::spawn_blocking` worker before
///   being dropped, so the potentially-blocking drain-thread join never
///   happens on the async runtime's worker threads.
///
/// Given those, marking the wrapper `Send + Sync` is sound. If a future
/// refactor removes the mutex guard or introduces parallel access, revisit
/// this.
pub struct SendCaptureHandle(pub Option<CaptureHandle>);

// SAFETY: see the doc comment above. Access is serialised through the
// `AppState::capture_handle` Mutex; the underlying platform audio APIs are
// thread-safe on macOS/Windows/Linux; and the !Send marker on cpal::Stream
// is defensive rather than reflecting a real platform constraint.
unsafe impl Send for SendCaptureHandle {}
unsafe impl Sync for SendCaptureHandle {}

/// Tracks the currently active recording session.
pub struct CurrentRecording {
    pub id: String,
    pub wav_path: PathBuf,
    pub started_at: Instant,
}


#[allow(dead_code)]
pub struct AppState {
    pub db: Arc<Database>,
    pub keys: Arc<KeyStorage>,
    pub data_dir: PathBuf,
    pub recording_active: Arc<Mutex<bool>>,
    pub ai_providers: Arc<Mutex<ProviderRegistry>>,
    pub stt_providers: Arc<Mutex<Option<Arc<dyn SttProvider + Send + Sync>>>>,
    pub orchestrator: Arc<AgentOrchestrator>,
    pub capture_handle: Arc<std::sync::Mutex<SendCaptureHandle>>,
    pub current_recording: Arc<std::sync::Mutex<Option<CurrentRecording>>>,
    /// Cancel tokens for in-flight pipelines, keyed by recording id. The
    /// pipeline inserts its own token on entry and removes it on exit;
    /// `cancel_pipeline` calls `.cancel()` to signal in-flight tasks and
    /// poll points to bail out.
    pub pipeline_cancels: Arc<std::sync::Mutex<HashMap<String, CancellationToken>>>,
    // RAG subsystem
    pub embedding_generator: Arc<EmbeddingGenerator>,
    pub vector_store: Arc<VectorStore>,
    pub bm25_search: Arc<Bm25Search>,
    pub graph_search: Arc<GraphSearch>,
    pub ingestion: Arc<IngestionPipeline>,
}

/// Register all supported AI providers (LM Studio + Ollama).
///
/// Both providers are local and keyless; `config` supplies LM Studio's
/// host/port.
pub fn init_ai_providers(config: &AppConfig) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    let policy = RetryConfig::from_app_config(config);

    // Ollama — always available (local, no key needed).
    // Builder failures are logged and the provider skipped rather than
    // crashing startup, so a weird system HTTP config doesn't brick the app.
    let ollama_host = if config.ollama_host.is_empty() { "localhost" } else { &config.ollama_host };
    let ollama_url = format!("http://{}:{}", ollama_host, config.ollama_port);
    match OllamaProvider::new(Some(&ollama_url), policy.clone()) {
        Ok(p) => {
            info!(url = %ollama_url, "Registering Ollama provider");
            registry.register(Arc::new(p));
        }
        Err(e) => tracing::error!(error = %e, url = %ollama_url, "Failed to build Ollama provider; skipping"),
    }

    // LM Studio — always available (local or remote, no key needed)
    let lmstudio_host = if config.lmstudio_host.is_empty() { "localhost" } else { &config.lmstudio_host };
    let lmstudio_url = format!("http://{}:{}", lmstudio_host, config.lmstudio_port);
    match LmStudioProvider::new(Some(&lmstudio_url), policy.clone()) {
        Ok(p) => {
            info!(url = %lmstudio_url, "Registering LM Studio provider");
            registry.register(Arc::new(p));
        }
        Err(e) => tracing::error!(error = %e, url = %lmstudio_url, "Failed to build LM Studio provider; skipping"),
    }

    info!("AI providers available: {:?}", registry.list_available());
    registry
}

/// Create the STT provider based on the user's chosen mode.
pub fn init_stt_providers_with_config(
    data_dir: &std::path::Path,
    config: &medical_core::types::settings::AppConfig,
) -> Option<Arc<dyn SttProvider + Send + Sync>> {
    let seg_path = stt_models::pyannote_model_path(data_dir, "segmentation-3.0.onnx");
    let emb_path = stt_models::pyannote_model_path(data_dir, "wespeaker_en_voxceleb_CAM++.onnx");

    match config.stt_mode {
        medical_core::types::settings::SttMode::Local => {
            let whisper_filename = stt_models::whisper_model_filename(&config.whisper_model)
                .unwrap_or("ggml-large-v3-turbo.bin");
            let whisper_path = stt_models::whisper_model_path(data_dir, whisper_filename);
            info!(
                whisper = %whisper_path.display(),
                segmentation = %seg_path.display(),
                embedding = %emb_path.display(),
                "Initializing local STT provider"
            );
            Some(Arc::new(medical_stt_providers::local_provider::LocalSttProvider::new(
                whisper_path,
                seg_path,
                emb_path,
            )))
        }
        medical_core::types::settings::SttMode::Remote => {
            // Load the remote API key from the keychain if present. A miss is
            // non-fatal — the provider just omits the Authorization header.
            // Mirror AppState::initialize's KeyStorage path: data_dir/config.
            let api_key = medical_security::key_storage::KeyStorage::open(&data_dir.join("config"))
                .ok()
                .and_then(|ks| ks.get_key("stt_remote_api_key").ok().flatten());

            info!(
                host = %config.stt_remote_host,
                port = config.stt_remote_port,
                model = %config.stt_remote_model,
                has_api_key = api_key.is_some(),
                "Initializing remote STT provider"
            );

            match medical_stt_providers::remote_provider::RemoteSttProvider::new(
                &config.stt_remote_host,
                config.stt_remote_port,
                &config.stt_remote_model,
                api_key,
                seg_path,
                emb_path,
            ) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to build remote STT provider");
                    None
                }
            }
        }
    }
}

impl AppState {
    pub fn initialize() -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rust-medical-assistant");
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("medical.db");
        let db = Database::open(&db_path)?;
        let db = Arc::new(db);

        // Flip any recordings that were Processing when the previous run ended
        // (crash, hard-quit, SIGKILL) to Failed so the UI doesn't show them
        // spinning forever.  Best-effort: a DB hiccup here shouldn't block boot.
        if let Ok(conn) = db.conn() {
            match RecordingsRepo::fail_stuck_processing(
                &conn,
                "Processing interrupted — app was closed before the pipeline finished.",
            ) {
                Ok(0) => {}
                Ok(n) => info!("Marked {n} stuck Processing recording(s) as Failed on boot"),
                Err(e) => tracing::warn!("fail_stuck_processing on boot failed: {e}"),
            }
        }

        let config_dir = data_dir.join("config");
        let keys = KeyStorage::open(&config_dir)?;

        // Load saved settings to configure preferred providers
        let config = {
            let conn = db.conn().ok();
            conn.and_then(|c| medical_db::settings::SettingsRepo::load_config(&c).ok())
                .map(|mut c| { c.migrate(); c })
        };
        let config_ref = config.as_ref().cloned().unwrap_or_default();

        // Initialize provider registries from saved API keys + config
        let mut ai_providers = init_ai_providers(&config_ref);

        let stt_providers = init_stt_providers_with_config(&data_dir, &config_ref);

        // Set the active AI provider from saved settings
        if let Some(ref cfg) = config {
            if ai_providers.set_active(&cfg.ai_provider) {
                info!("Active AI provider set to '{}' from settings", cfg.ai_provider);
            }
        }

        // --- RAG subsystem ---
        let embedding_host = if config_ref.ollama_host.is_empty() {
            "localhost".to_string()
        } else {
            config_ref.ollama_host.clone()
        };
        let embedding_url = format!("http://{}:{}", embedding_host, config_ref.ollama_port);
        info!(url = %embedding_url, model = %config_ref.embedding_model, "RAG: using Ollama embeddings");
        let embedding_generator = Arc::new(EmbeddingGenerator::new_ollama(
            Some(&embedding_url),
            Some(&config_ref.embedding_model),
        ));

        let vector_store = Arc::new(VectorStore::new(Arc::clone(&db)));
        let bm25_search = Arc::new(Bm25Search::new(Arc::clone(&db)));
        let graph_search = Arc::new(GraphSearch::new(Arc::clone(&db)));
        let ingestion = Arc::new(IngestionPipeline::new(
            Arc::clone(&embedding_generator),
            Arc::clone(&vector_store),
            Arc::clone(&graph_search),
        ));

        info!("RAG subsystem initialized");

        // Initialize the agent orchestrator with tools, including RAG-connected search
        let mut tool_registry = ToolRegistry::with_defaults();
        // Replace the default unconfigured RagSearchTool with a real one
        let rag_tool = RagSearchTool::with_rag(
            Arc::clone(&embedding_generator),
            Arc::clone(&vector_store),
            Arc::clone(&bm25_search),
        );
        tool_registry.register(Arc::new(rag_tool));
        let orchestrator = AgentOrchestrator::new(tool_registry);

        Ok(Self {
            db,
            keys: Arc::new(keys),
            data_dir,
            recording_active: Arc::new(Mutex::new(false)),
            ai_providers: Arc::new(Mutex::new(ai_providers)),
            stt_providers: Arc::new(Mutex::new(stt_providers)),
            orchestrator: Arc::new(orchestrator),
            capture_handle: Arc::new(std::sync::Mutex::new(SendCaptureHandle(None))),
            current_recording: Arc::new(std::sync::Mutex::new(None)),
            pipeline_cancels: Arc::new(std::sync::Mutex::new(HashMap::new())),
            embedding_generator,
            vector_store,
            bm25_search,
            graph_search,
            ingestion,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::settings::AppConfig;

    #[test]
    fn init_ai_providers_uses_configured_ollama_host() {
        let mut config = AppConfig::default();
        config.ollama_host = "tailnet-node".into();
        config.ollama_port = 11500;
        let registry = init_ai_providers(&config);
        assert!(
            registry.list_available().contains(&"ollama".to_string()),
            "ollama should still be registered with a custom host"
        );
    }

    #[test]
    fn init_stt_providers_remote_mode_builds_remote_provider() {
        use medical_core::types::settings::{AppConfig, SttMode};
        let mut cfg = AppConfig::default();
        cfg.stt_mode = SttMode::Remote;
        cfg.stt_remote_host = "tailnet-node".into();
        cfg.stt_remote_port = 8080;
        cfg.stt_remote_model = "whisper-1".into();

        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = init_stt_providers_with_config(tmp.path(), &cfg)
            .expect("provider should be built");
        assert_eq!(provider.name(), "remote");
    }

    #[test]
    fn init_stt_providers_local_mode_builds_local_provider() {
        use medical_core::types::settings::{AppConfig, SttMode};
        let mut cfg = AppConfig::default();
        cfg.stt_mode = SttMode::Local;
        cfg.whisper_model = "large-v3-turbo".into();

        let tmp = tempfile::tempdir().expect("tempdir");
        let provider = init_stt_providers_with_config(tmp.path(), &cfg)
            .expect("provider should be built");
        assert_eq!(provider.name(), "local");
    }
}
