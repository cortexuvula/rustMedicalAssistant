use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::info;

use medical_ai_providers::ollama::OllamaProvider;
use medical_ai_providers::lmstudio::LmStudioProvider;
use medical_ai_providers::ProviderRegistry;

use medical_core::types::settings::AppConfig;

use medical_agents::orchestrator::AgentOrchestrator;
use medical_agents::tools::{RagSearchTool, ToolRegistry};

use medical_audio::capture::CaptureHandle;

use medical_db::Database;

use medical_rag::bm25::Bm25Search;
use medical_rag::embeddings::EmbeddingGenerator;
use medical_rag::graph_search::GraphSearch;
use medical_rag::ingestion::IngestionPipeline;
use medical_rag::vector_store::VectorStore;

use medical_security::key_storage::KeyStorage;

use medical_stt_providers::LocalSttProvider;
use medical_stt_providers::models as stt_models;

use medical_core::traits::SttProvider;

/// Wrapper to make `CaptureHandle` usable across threads.
///
/// `CaptureHandle` holds a `cpal::Stream` which is marked `!Send` on some
/// platforms as a conservative safety measure.  In practice the handle's
/// `pause`/`resume`/`stop` methods only set atomic flags or join a thread,
/// so cross-thread access is safe.  We gate all access behind a
/// `std::sync::Mutex` to serialize callers.
pub struct SendCaptureHandle(pub Option<CaptureHandle>);

// SAFETY: Access is serialized through a std::sync::Mutex in AppState.
// The CaptureHandle methods (pause/resume/stop) only touch AtomicBool flags
// and a JoinHandle, which are inherently thread-safe.
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
    /// Cancel flags for in-flight pipelines, keyed by recording id. The
    /// pipeline inserts its own flag on entry and removes it on exit;
    /// `cancel_pipeline` flips a flag to signal the poll points to bail out.
    pub pipeline_cancels: Arc<std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>>,
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

    // Ollama — always available (local, no key needed)
    info!("Registering Ollama provider (local)");
    registry.register(Arc::new(OllamaProvider::new(None)));

    // LM Studio — always available (local or remote, no key needed)
    let lmstudio_host = if config.lmstudio_host.is_empty() { "localhost" } else { &config.lmstudio_host };
    let lmstudio_url = format!("http://{}:{}", lmstudio_host, config.lmstudio_port);
    info!(url = %lmstudio_url, "Registering LM Studio provider");
    registry.register(Arc::new(LmStudioProvider::new(Some(&lmstudio_url))));

    info!("AI providers available: {:?}", registry.list_available());
    registry
}

/// Create the local STT provider with model paths from the app data directory.
pub fn init_stt_providers(data_dir: &std::path::Path, whisper_model_id: &str) -> Option<Arc<dyn SttProvider + Send + Sync>> {
    let whisper_filename = stt_models::whisper_model_filename(whisper_model_id)
        .unwrap_or("ggml-large-v3-turbo.bin");

    let whisper_path = stt_models::whisper_model_path(data_dir, whisper_filename);
    let seg_path = stt_models::pyannote_model_path(data_dir, "segmentation-3.0.onnx");
    let emb_path = stt_models::pyannote_model_path(data_dir, "wespeaker_en_voxceleb_CAM++.onnx");

    info!(
        whisper = %whisper_path.display(),
        segmentation = %seg_path.display(),
        embedding = %emb_path.display(),
        "Initializing local STT provider"
    );

    let provider = LocalSttProvider::new(whisper_path, seg_path, emb_path);
    Some(Arc::new(provider))
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

        let whisper_model = config.as_ref()
            .map(|c| c.whisper_model.as_str())
            .unwrap_or("large-v3-turbo");
        let stt_providers = init_stt_providers(&data_dir, whisper_model);

        // Set the active AI provider from saved settings
        if let Some(ref cfg) = config {
            if ai_providers.set_active(&cfg.ai_provider) {
                info!("Active AI provider set to '{}' from settings", cfg.ai_provider);
            }
        }

        // --- RAG subsystem ---
        info!("RAG: using Ollama embeddings (local)");
        let embedding_generator = Arc::new(EmbeddingGenerator::new_ollama(None, None));

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
