use anyhow::{Context, Result};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Process-wide configuration, loaded once at startup.
///
/// Every value has a default so Lumen runs with no config file; `LUMEN_CONFIG`
/// or `config/lumen.toml` overrides selected fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Address the HTTP control/view plane binds to. Keep it loopback.
    pub host: String,
    /// Port for the HTTP control/view plane.
    pub port: u16,
    /// Maximum concurrently running agent browsers.
    pub max_agents: usize,
    /// Per-agent Chromium profile root inside the container.
    pub data_dir: PathBuf,
    /// Chromium binary executed by the supervisor.
    pub chrome_bin: String,
    /// Viewport every browser starts at, before any explicit override.
    pub default_viewport: Viewport,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8899,
            max_agents: 8,
            data_dir: PathBuf::from("/data/agents"),
            chrome_bin: "chromium".into(),
            default_viewport: Viewport {
                width: 1440,
                height: 900,
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path =
            std::env::var("LUMEN_CONFIG").unwrap_or_else(|_| "config/lumen.toml".to_string());
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let config: Config =
                    toml::from_str(&text).with_context(|| format!("invalid config at {path}"))?;
                Ok(config)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(err) => Err(err).with_context(|| format!("failed to read {path}")),
        }
    }

    pub fn bind_addr(&self) -> SocketAddr {
        let raw = format!("{}:{}", self.host, self.port);
        raw.parse().expect("host/port must form a socket address")
    }
}
