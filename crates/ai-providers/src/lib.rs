pub mod http_client;
pub mod sse;
pub mod openai_compat;
pub mod openai;
pub mod anthropic;
pub mod gemini;
pub mod groq;
pub mod cerebras;
pub mod ollama;
pub mod lmstudio;

use std::collections::HashMap;
use std::sync::Arc;
use medical_core::traits::AiProvider;

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn AiProvider>>,
    active: String,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self { providers: HashMap::new(), active: String::new() }
    }
    pub fn register(&mut self, provider: Arc<dyn AiProvider>) {
        let name = provider.name().to_string();
        if self.active.is_empty() { self.active = name.clone(); }
        self.providers.insert(name, provider);
    }
    pub fn get(&self, name: &str) -> Option<&dyn AiProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }
    pub fn active(&self) -> Option<&dyn AiProvider> { self.get(&self.active) }
    /// Returns the name of the currently active provider.
    pub fn active_name(&self) -> &str { &self.active }
    /// Returns a cloned `Arc` of a named provider, suitable for use across await points.
    pub fn get_arc(&self, name: &str) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(name).cloned()
    }
    /// Returns a cloned `Arc` of the active provider, suitable for use across await points.
    pub fn get_active_arc(&self) -> Option<Arc<dyn AiProvider>> {
        self.providers.get(&self.active).cloned()
    }
    pub fn set_active(&mut self, name: &str) -> bool {
        if self.providers.contains_key(name) { self.active = name.to_string(); true } else { false }
    }
    pub fn list_available(&self) -> Vec<String> { self.providers.keys().cloned().collect() }
}
