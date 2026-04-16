use serde::Deserialize;
use std::path::PathBuf;

/// Configuration loaded from `config.toml` next to the binary,
/// or from `~/.config/efact-printer-agent/config.toml` as fallback.
/// All fields have sensible defaults so the agent works out-of-the-box.
#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    /// HTTP port to listen on. Default: 8765
    #[serde(default = "default_port")]
    pub port: u16,

    /// Specific USB vendor_id to target (hex string, e.g. "04b8" for Epson).
    /// If None, the agent tries all known thermal printer vendor IDs.
    pub usb_vendor_id: Option<String>,

    /// Specific USB product_id to target (hex string).
    /// If None, the first matching device is used.
    pub usb_product_id: Option<String>,

    /// USB output endpoint address. Default: 0x01 (most thermal printers).
    #[serde(default = "default_endpoint")]
    pub usb_endpoint: u8,

    /// Chunk size in bytes when writing to USB. Default: 4096
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,

    /// Optional system printer name to target through the OS print spooler.
    /// If omitted, the default system printer is used.
    pub system_printer_name: Option<String>,

    /// Prefer the system print backend before trying USB HID.
    #[serde(default)]
    pub prefer_system_backend: bool,
}

fn default_port() -> u16 {
    8765
}

fn default_endpoint() -> u8 {
    0x01
}

fn default_chunk_size() -> usize {
    4096
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            usb_vendor_id: None,
            usb_product_id: None,
            usb_endpoint: default_endpoint(),
            chunk_size: default_chunk_size(),
            system_printer_name: None,
            prefer_system_backend: false,
        }
    }
}

impl AgentConfig {
    pub fn load() -> Self {
        // 1. Next to the binary
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("config.toml")));

        // 2. ~/.config/efact-printer-agent/config.toml
        let user_config = dirs_config_path();

        let candidates = [exe_dir, user_config]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        for path in &candidates {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(contents) => match toml::from_str::<AgentConfig>(&contents) {
                        Ok(cfg) => {
                            tracing::info!("Loaded config from {}", path.display());
                            return cfg;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse {}: {e}", path.display());
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to read {}: {e}", path.display());
                    }
                }
            }
        }

        tracing::info!("No config.toml found, using defaults");
        AgentConfig::default()
    }
}

fn dirs_config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(|d| {
            PathBuf::from(d)
                .join("efact-printer-agent")
                .join("config.toml")
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("efact-printer-agent")
                .join("config.toml")
        })
    }
}
