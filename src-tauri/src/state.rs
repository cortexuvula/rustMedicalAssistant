use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::info;

use medical_ai_providers::openai::OpenAiProvider;
use medical_ai_providers::anthropic::AnthropicProvider;
use medical_ai_providers::gemini::GeminiProvider;
use medical_ai_providers::groq::GroqProvider;
use medical_ai_providers::cerebras::CerebrasProvider;
use medical_ai_providers::ollama::OllamaProvider;
use medical_ai_providers::ProviderRegistry;

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

use medical_stt_providers::deepgram::DeepgramProvider;
use medical_stt_providers::elevenlabs_stt::ElevenLabsSttProvider;
use medical_stt_providers::failover::SttFailover;
use medical_stt_providers::groq_whisper::GroqWhisperProvider;

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
    pub stt_providers: Arc<Mutex<Option<Arc<SttFailover>>>>,
    pub orchestrator: Arc<AgentOrchestrator>,
    pub capture_handle: Arc<std::sync::Mutex<SendCaptureHandle>>,
    pub current_recording: Arc<std::sync::Mutex<Option<CurrentRecording>>>,
    // RAG subsystem
    pub embedding_generator: Arc<EmbeddingGenerator>,
    pub vector_store: Arc<VectorStore>,
    pub bm25_search: Arc<Bm25Search>,
    pub graph_search: Arc<GraphSearch>,
    pub ingestion: Arc<IngestionPipeline>,
}

/// Read saved API keys and register all available AI providers.
pub fn init_ai_providers(keys: &KeyStorage) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    // OpenAI
    if let Ok(Some(key)) = keys.get_key("openai") {
        info!("Registering OpenAI provider");
        registry.register(Arc::new(OpenAiProvider::new(&key)));
    }

    // Anthropic
    if let Ok(Some(key)) = keys.get_key("anthropic") {
        info!("Registering Anthropic provider");
        registry.register(Arc::new(AnthropicProvider::new(&key)));
    }

    // Gemini
    if let Ok(Some(key)) = keys.get_key("gemini") {
        info!("Registering Gemini provider");
        registry.register(Arc::new(GeminiProvider::new(&key)));
    }

    // Groq
    if let Ok(Some(key)) = keys.get_key("groq") {
        info!("Registering Groq provider");
        registry.register(Arc::new(GroqProvider::new(&key)));
    }

    // Cerebras
    if let Ok(Some(key)) = keys.get_key("cerebras") {
        info!("Registering Cerebras provider");
        registry.register(Arc::new(CerebrasProvider::new(&key)));
    }

    // Ollama — always available (local, no key needed)
    info!("Registering Ollama provider (local)");
    registry.register(Arc::new(OllamaProvider::new(None)));

    info!("AI providers available: {:?}", registry.list_available());
    registry
}

/// Read saved API keys and build an STT failover chain from available providers.
///
/// Returns `None` if no STT provider keys are configured.
pub fn init_stt_providers(keys: &KeyStorage) -> Option<SttFailover> {
    let mut chain: Vec<Arc<dyn SttProvider>> = Vec::new();

    // Deepgram — preferred for medical transcription
    if let Ok(Some(key)) = keys.get_key("deepgram") {
        match DeepgramProvider::new(&key) {
            Ok(provider) => {
                info!("Adding Deepgram to STT failover chain");
                chain.push(Arc::new(provider));
            }
            Err(e) => {
                tracing::warn!("Failed to create Deepgram STT provider: {e}");
            }
        }
    }

    // Groq Whisper
    if let Ok(Some(key)) = keys.get_key("groq") {
        match GroqWhisperProvider::new(&key) {
            Ok(provider) => {
                info!("Adding Groq Whisper to STT failover chain");
                chain.push(Arc::new(provider));
            }
            Err(e) => {
                tracing::warn!("Failed to create Groq Whisper STT provider: {e}");
            }
        }
    }

    // ElevenLabs
    if let Ok(Some(key)) = keys.get_key("elevenlabs") {
        match ElevenLabsSttProvider::new(&key) {
            Ok(provider) => {
                info!("Adding ElevenLabs to STT failover chain");
                chain.push(Arc::new(provider));
            }
            Err(e) => {
                tracing::warn!("Failed to create ElevenLabs STT provider: {e}");
            }
        }
    }

    if chain.is_empty() {
        info!("No STT providers configured");
        None
    } else {
        info!("STT failover chain has {} provider(s)", chain.len());
        Some(SttFailover::new(chain))
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

        let config_dir = data_dir.join("config");
        let keys = KeyStorage::open(&config_dir)?;

        // Initialize provider registries from saved API keys
        let ai_providers = init_ai_providers(&keys);
        let stt_providers = init_stt_providers(&keys);

        // --- RAG subsystem ---
        // Create the embedding generator: prefer OpenAI if key exists, else Ollama
        let embedding_generator = if let Ok(Some(key)) = keys.get_key("openai") {
            info!("RAG: using OpenAI embeddings");
            Arc::new(EmbeddingGenerator::new_openai(&key))
        } else {
            info!("RAG: using Ollama embeddings (local)");
            Arc::new(EmbeddingGenerator::new_ollama(None, None))
        };

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
            stt_providers: Arc::new(Mutex::new(stt_providers.map(Arc::new))),
            orchestrator: Arc::new(orchestrator),
            capture_handle: Arc::new(std::sync::Mutex::new(SendCaptureHandle(None))),
            current_recording: Arc::new(std::sync::Mutex::new(None)),
            embedding_generator,
            vector_store,
            bm25_search,
            graph_search,
            ingestion,
        })
    }
}
