//! Orchestrator — owns lifecycle of all sharing subsystems.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharingConfig {
    pub enabled: bool,
    pub friendly_name: Option<String>,
    pub ollama_proxy_port: u16,
    pub whisper_proxy_port: u16,
    pub pairing_port: u16,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SharingStatus {
    pub enabled: bool,
    pub ollama_ok: bool,
    pub whisper_ok: bool,
    pub mdns_ok: bool,
    pub pairing_ok: bool,
    pub paired_clients: u32,
}

pub struct SharingService;

impl SharingService {
    pub fn new(_config: SharingConfig) -> crate::Result<Self> {
        Ok(Self)
    }

    pub async fn start(&self) -> crate::Result<()> { Ok(()) }
    pub async fn stop(&self) -> crate::Result<()> { Ok(()) }
    pub fn status(&self) -> SharingStatus { SharingStatus::default() }
}
