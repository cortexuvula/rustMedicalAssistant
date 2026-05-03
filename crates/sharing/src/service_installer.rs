//! Service installer — per-platform persistent Ollama service writers (launchd / systemd / scheduled task).
//!
//! Writes a service unit that runs `ollama serve` with
//! OLLAMA_HOST=127.0.0.1:11434 so Ollama remains loopback-only. The auth
//! proxy is what's exposed on the network.

use std::path::PathBuf;

use crate::SharingError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Installed,
    Missing,
    UnknownPlatform,
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    fn plist_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library/LaunchAgents/com.ferriscribe.ollama.plist")
    }

    pub fn install() -> Result<(), SharingError> {
        let path = plist_path();
        let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.ferriscribe.ollama</string>
  <key>ProgramArguments</key>
  <array><string>/usr/local/bin/ollama</string><string>serve</string></array>
  <key>EnvironmentVariables</key>
  <dict><key>OLLAMA_HOST</key><string>127.0.0.1:11434</string></dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
"#;
        std::fs::create_dir_all(path.parent().unwrap())
            .map_err(SharingError::Io)?;
        std::fs::write(&path, plist).map_err(SharingError::Io)?;
        let status = std::process::Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&path)
            .status()
            .map_err(SharingError::Io)?;
        if !status.success() {
            return Err(SharingError::ServiceInstaller(format!(
                "launchctl load exited with {}", status
            )));
        }
        Ok(())
    }

    pub fn state() -> ServiceState {
        if plist_path().exists() {
            ServiceState::Installed
        } else {
            ServiceState::Missing
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    fn unit_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("systemd/user/ferriscribe-ollama.service")
    }

    pub fn install() -> Result<(), SharingError> {
        let path = unit_path();
        let unit = r#"[Unit]
Description=Ollama (managed by FerriScribe)

[Service]
Environment=OLLAMA_HOST=127.0.0.1:11434
ExecStart=/usr/local/bin/ollama serve
Restart=on-failure

[Install]
WantedBy=default.target
"#;
        std::fs::create_dir_all(path.parent().unwrap())
            .map_err(SharingError::Io)?;
        std::fs::write(&path, unit).map_err(SharingError::Io)?;
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", "ferriscribe-ollama.service"])
            .status();
        Ok(())
    }

    pub fn state() -> ServiceState {
        if unit_path().exists() {
            ServiceState::Installed
        } else {
            ServiceState::Missing
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub fn install() -> Result<(), SharingError> {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>
  <Actions Context="Author">
    <Exec>
      <Command>cmd.exe</Command>
      <Arguments>/c set OLLAMA_HOST=127.0.0.1:11434 &amp; ollama.exe serve</Arguments>
    </Exec>
  </Actions>
</Task>"#;
        let dir = std::env::temp_dir();
        let xml_path = dir.join("ferriscribe-ollama.xml");
        std::fs::write(&xml_path, xml).map_err(SharingError::Io)?;
        let status = std::process::Command::new("schtasks")
            .args(["/Create", "/TN", "FerriScribe Ollama", "/XML"])
            .arg(&xml_path)
            .args(["/F"])
            .status()
            .map_err(SharingError::Io)?;
        if !status.success() {
            return Err(SharingError::ServiceInstaller(format!(
                "schtasks /Create exited with {}", status
            )));
        }
        Ok(())
    }

    pub fn state() -> ServiceState {
        let out = std::process::Command::new("schtasks")
            .args(["/Query", "/TN", "FerriScribe Ollama"])
            .output();
        match out {
            Ok(o) if o.status.success() => ServiceState::Installed,
            Ok(_) => ServiceState::Missing,
            Err(_) => ServiceState::UnknownPlatform,
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::{install as install_persistent_ollama, state as ollama_service_state};
#[cfg(target_os = "linux")]
pub use linux::{install as install_persistent_ollama, state as ollama_service_state};
#[cfg(target_os = "windows")]
pub use windows::{install as install_persistent_ollama, state as ollama_service_state};

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn install_persistent_ollama() -> Result<(), SharingError> {
    Err(SharingError::ServiceInstaller("unsupported platform".into()))
}
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn ollama_service_state() -> ServiceState { ServiceState::UnknownPlatform }
