use medical_core::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Which embedding backend to use for vector generation.
enum EmbeddingBackend {
    OpenAi { api_key: String },
    Ollama { host: String, model: String },
}

/// HTTP-backed embedding generator supporting OpenAI and Ollama.
pub struct EmbeddingGenerator {
    client: Client,
    backend: EmbeddingBackend,
    dim: usize,
}

// ---------------------------------------------------------------------------
// OpenAI request/response shapes
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    data: Vec<OpenAiEmbedding>,
}

#[derive(Deserialize)]
struct OpenAiEmbedding {
    embedding: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Ollama request/response shapes
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct OllamaResponse {
    embedding: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl EmbeddingGenerator {
    /// Create a generator backed by OpenAI's `text-embedding-3-small` model (1536 dims).
    pub fn new_openai(api_key: &str) -> Self {
        Self {
            client: Client::new(),
            backend: EmbeddingBackend::OpenAi {
                api_key: api_key.to_owned(),
            },
            dim: 1536,
        }
    }

    /// Create a generator backed by a local Ollama instance.
    ///
    /// Defaults to `http://localhost:11434` and the `nomic-embed-text` model (768 dims).
    pub fn new_ollama(host: Option<&str>, model: Option<&str>) -> Self {
        Self {
            client: Client::new(),
            backend: EmbeddingBackend::Ollama {
                host: host.unwrap_or("http://localhost:11434").to_owned(),
                model: model.unwrap_or("nomic-embed-text").to_owned(),
            },
            dim: 768,
        }
    }

    /// The dimensionality of the vectors produced by this generator.
    pub fn dimension(&self) -> usize {
        self.dim
    }

    /// Generate an embedding for a single text.
    pub async fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
        match &self.backend {
            EmbeddingBackend::OpenAi { api_key } => {
                let body = OpenAiRequest {
                    model: "text-embedding-3-small",
                    input: vec![text],
                };
                let resp = self
                    .client
                    .post("https://api.openai.com/v1/embeddings")
                    .header("Authorization", format!("Bearer {api_key}"))
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AppError::AiProvider(format!("OpenAI request failed: {e}")))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body_text = resp.text().await.unwrap_or_default();
                    return Err(AppError::AiProvider(format!(
                        "OpenAI API error {status}: {body_text}"
                    )));
                }

                let parsed: OpenAiResponse = resp
                    .json()
                    .await
                    .map_err(|e| AppError::AiProvider(format!("OpenAI parse error: {e}")))?;

                parsed
                    .data
                    .into_iter()
                    .next()
                    .map(|d| d.embedding)
                    .ok_or_else(|| AppError::AiProvider("OpenAI returned no embeddings".into()))
            }

            EmbeddingBackend::Ollama { host, model } => {
                let body = OllamaRequest {
                    model: model.as_str(),
                    prompt: text,
                };
                let url = format!("{host}/api/embeddings");
                let resp = self
                    .client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AppError::AiProvider(format!("Ollama request failed: {e}")))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body_text = resp.text().await.unwrap_or_default();
                    return Err(AppError::AiProvider(format!(
                        "Ollama API error {status}: {body_text}"
                    )));
                }

                let parsed: OllamaResponse = resp
                    .json()
                    .await
                    .map_err(|e| AppError::AiProvider(format!("Ollama parse error: {e}")))?;

                Ok(parsed.embedding)
            }
        }
    }

    /// Generate embeddings for a batch of texts.
    ///
    /// OpenAI supports native batching; Ollama requires one call per text.
    pub async fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        match &self.backend {
            EmbeddingBackend::OpenAi { api_key } => {
                let body = OpenAiRequest {
                    model: "text-embedding-3-small",
                    input: texts.to_vec(),
                };
                let resp = self
                    .client
                    .post("https://api.openai.com/v1/embeddings")
                    .header("Authorization", format!("Bearer {api_key}"))
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AppError::AiProvider(format!("OpenAI request failed: {e}")))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body_text = resp.text().await.unwrap_or_default();
                    return Err(AppError::AiProvider(format!(
                        "OpenAI API error {status}: {body_text}"
                    )));
                }

                let parsed: OpenAiResponse = resp
                    .json()
                    .await
                    .map_err(|e| AppError::AiProvider(format!("OpenAI parse error: {e}")))?;

                Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
            }

            EmbeddingBackend::Ollama { host, model } => {
                let url = format!("{host}/api/embeddings");
                let mut results = Vec::with_capacity(texts.len());
                for text in texts {
                    let body = OllamaRequest {
                        model: model.as_str(),
                        prompt: text,
                    };
                    let resp = self
                        .client
                        .post(&url)
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| {
                            AppError::AiProvider(format!("Ollama request failed: {e}"))
                        })?;

                    if !resp.status().is_success() {
                        let status = resp.status();
                        let body_text = resp.text().await.unwrap_or_default();
                        return Err(AppError::AiProvider(format!(
                            "Ollama API error {status}: {body_text}"
                        )));
                    }

                    let parsed: OllamaResponse = resp
                        .json()
                        .await
                        .map_err(|e| {
                            AppError::AiProvider(format!("Ollama parse error: {e}"))
                        })?;
                    results.push(parsed.embedding);
                }
                Ok(results)
            }
        }
    }
}

/// Default creates a local Ollama backend (local-first experience).
impl Default for EmbeddingGenerator {
    fn default() -> Self {
        Self::new_ollama(None, None)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_constructor() {
        let emb = EmbeddingGenerator::new_openai("sk-test-key");
        assert_eq!(emb.dimension(), 1536);
    }

    #[test]
    fn ollama_constructor_defaults() {
        let emb = EmbeddingGenerator::new_ollama(None, None);
        assert_eq!(emb.dimension(), 768);
        match &emb.backend {
            EmbeddingBackend::Ollama { host, model } => {
                assert_eq!(host, "http://localhost:11434");
                assert_eq!(model, "nomic-embed-text");
            }
            _ => panic!("expected Ollama backend"),
        }
    }

    #[test]
    fn ollama_constructor_custom() {
        let emb = EmbeddingGenerator::new_ollama(
            Some("http://myhost:1234"),
            Some("custom-model"),
        );
        assert_eq!(emb.dimension(), 768);
        match &emb.backend {
            EmbeddingBackend::Ollama { host, model } => {
                assert_eq!(host, "http://myhost:1234");
                assert_eq!(model, "custom-model");
            }
            _ => panic!("expected Ollama backend"),
        }
    }

    #[test]
    fn default_is_ollama() {
        let emb = EmbeddingGenerator::default();
        assert_eq!(emb.dimension(), 768);
        assert!(matches!(emb.backend, EmbeddingBackend::Ollama { .. }));
    }
}
