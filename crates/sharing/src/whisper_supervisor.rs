//! whisper.cpp child process supervisor + on-demand binary download.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{info, warn};

const MANIFEST: &str = include_str!("../whisper-manifest.json");

#[derive(Debug, Deserialize)]
struct Manifest {
    #[allow(dead_code)]
    version: String,
    binaries: std::collections::HashMap<String, BinaryEntry>,
}

#[derive(Debug, Deserialize)]
struct BinaryEntry {
    url: String,
    sha256: Option<String>,
    archive: String,
    binary_name: String,
}

fn platform_key() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => "unsupported",
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WhisperError {
    #[error("platform unsupported")]
    UnsupportedPlatform,
    #[error("download: {0}")]
    Download(String),
    #[error("hash mismatch (expected {expected}, got {got})")]
    HashMismatch { expected: String, got: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest: {0}")]
    Manifest(String),
}

pub type Result<T> = std::result::Result<T, WhisperError>;

pub struct WhisperSupervisor {
    binary_dir: PathBuf,
    model_path: PathBuf,
    api_key: String,
    port: u16,
    child: Mutex<Option<Child>>,
    stop: Arc<tokio::sync::Notify>,
}

impl WhisperSupervisor {
    pub fn new(binary_dir: PathBuf, model_path: PathBuf, port: u16, api_key: String) -> Self {
        Self {
            binary_dir,
            model_path,
            port,
            api_key,
            child: Mutex::new(None),
            stop: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub async fn ensure_binary(&self) -> Result<PathBuf> {
        let manifest: Manifest =
            serde_json::from_str(MANIFEST).map_err(|e| WhisperError::Manifest(e.to_string()))?;
        let key = platform_key();
        let entry = manifest
            .binaries
            .get(key)
            .ok_or(WhisperError::UnsupportedPlatform)?;
        let bin_path = self.binary_dir.join(&entry.binary_name);
        if bin_path.exists() {
            return Ok(bin_path);
        }
        tokio::fs::create_dir_all(&self.binary_dir).await?;
        let bytes = reqwest::get(&entry.url)
            .await
            .map_err(|e| WhisperError::Download(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| WhisperError::Download(e.to_string()))?;
        if let Some(expected) = &entry.sha256 {
            let got = hex::encode(Sha256::digest(&bytes));
            if &got != expected {
                return Err(WhisperError::HashMismatch {
                    expected: expected.clone(),
                    got,
                });
            }
        } else {
            warn!("sha256 not set for platform {}; skipping verification", key);
        }
        Self::extract_archive(&bytes, &entry.archive, &self.binary_dir, &entry.binary_name)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms)?;
        }
        Ok(bin_path)
    }

    fn extract_archive(
        bytes: &[u8],
        archive_kind: &str,
        out_dir: &Path,
        binary_name: &str,
    ) -> Result<()> {
        match archive_kind {
            "zip" => extract_zip(bytes, out_dir, binary_name),
            "tar.gz" => extract_tar_gz(bytes, out_dir, binary_name),
            other => Err(WhisperError::Manifest(format!(
                "unsupported archive: {other}"
            ))),
        }
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        let bin = self.ensure_binary().await?;
        let child = self.spawn_once_at(&bin).await?;
        *self.child.lock().await = Some(child);
        let me = Arc::clone(self);
        tokio::spawn(async move {
            me.supervise().await;
        });
        Ok(())
    }

    async fn supervise(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            let mut guard = self.child.lock().await;
            let Some(mut c) = guard.take() else { return; };
            drop(guard);
            tokio::select! {
                _ = c.wait() => {
                    info!("whisper-server exited; restarting in {:?}", backoff);
                    // Wait for the backoff period, but bail immediately if stop fires.
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {}
                        _ = self.stop.notified() => { return; }
                    }
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                    let bin = match self.binary_dir.read_dir() {
                        Ok(_) => self.binary_dir.join(self.binary_name_for_platform()),
                        Err(_) => return,
                    };
                    if let Ok(child) = self.spawn_once_at(&bin).await {
                        *self.child.lock().await = Some(child);
                    } else {
                        return;
                    }
                }
                _ = self.stop.notified() => {
                    let _ = c.kill().await;
                    return;
                }
            }
        }
    }

    fn binary_name_for_platform(&self) -> &'static str {
        // Defaults that match the manifest.
        if cfg!(target_os = "windows") { "whisper-server.exe" } else { "whisper-server" }
    }

    async fn spawn_once_at(&self, bin: &Path) -> Result<Child> {
        let mut cmd = Command::new(bin);
        cmd.arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(self.port.to_string())
            .arg("-m")
            .arg(&self.model_path)
            .arg("--api-key")
            .arg(&self.api_key)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Ok(cmd.spawn()?)
    }

    pub async fn stop(&self) {
        self.stop.notify_waiters();
        if let Some(mut c) = self.child.lock().await.take() {
            let _ = c.kill().await;
        }
    }
}

fn extract_zip(bytes: &[u8], out_dir: &Path, binary_name: &str) -> Result<()> {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip =
        zip::ZipArchive::new(cursor).map_err(|e| WhisperError::Manifest(e.to_string()))?;
    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| WhisperError::Manifest(e.to_string()))?;
        let name = file.name().to_string();
        if Path::new(&name).file_name().and_then(|s| s.to_str()) == Some(binary_name) {
            let mut buf = Vec::with_capacity(file.size() as usize);
            std::io::Read::read_to_end(&mut file, &mut buf)?;
            std::fs::write(out_dir.join(binary_name), buf)?;
            return Ok(());
        }
    }
    Err(WhisperError::Manifest(format!(
        "binary {binary_name} not found in zip"
    )))
}

fn extract_tar_gz(bytes: &[u8], out_dir: &Path, binary_name: &str) -> Result<()> {
    let cursor = std::io::Cursor::new(bytes);
    let gz = flate2::read::GzDecoder::new(cursor);
    let mut ar = tar::Archive::new(gz);
    for entry in ar.entries()? {
        let mut e = entry?;
        let path = e.path()?.to_path_buf();
        if path.file_name().and_then(|s| s.to_str()) == Some(binary_name) {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut e, &mut buf)?;
            std::fs::write(out_dir.join(binary_name), buf)?;
            return Ok(());
        }
    }
    Err(WhisperError::Manifest(format!(
        "binary {binary_name} not found in tar.gz"
    )))
}
