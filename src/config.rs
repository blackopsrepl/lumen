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
    /// SQLite database holding human feedback for each session.
    pub feedback_db: PathBuf,
    /// Maximum audit rows to retain.
    pub audit_retain: i64,
    /// Navigation host policy.
    pub policy: Policy,
}

/// Host-level navigation policy.
///
/// An empty `allow_hosts` permits every host; otherwise only listed hosts are
/// reachable. `blocked_hosts` always wins. Entries beginning with `.` match a
/// domain suffix (`.example.com` allows `api.example.com`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub allow_hosts: Vec<String>,
    pub blocked_hosts: Vec<String>,
}

/// Extract the host of a URL, or `None` for non-http(s) schemes.
pub fn url_host(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    match parsed.scheme() {
        "http" | "https" => parsed.host_str().map(str::to_string),
        _ => None,
    }
}

fn host_matches(host: &str, entry: &str) -> bool {
    if let Some(suffix) = entry.strip_prefix('.') {
        host == suffix || host.ends_with(&format!(".{suffix}"))
    } else {
        host == entry
    }
}

impl Policy {
    /// Reject a navigation that the policy forbids. Non-http(s) URLs (data:,
    /// about:, file: …) are allowed through, since they cannot reach a host.
    pub fn check(&self, url: &str) -> anyhow::Result<()> {
        let Some(host) = url_host(url) else {
            return Ok(());
        };
        if self.blocked_hosts.iter().any(|e| host_matches(&host, e)) {
            anyhow::bail!("host '{host}' is blocked by policy");
        }
        if !self.allow_hosts.is_empty() && !self.allow_hosts.iter().any(|e| host_matches(&host, e))
        {
            anyhow::bail!("host '{host}' is not in the allow list");
        }
        Ok(())
    }
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
            feedback_db: PathBuf::from("/data/feedback.db"),
            audit_retain: 10_000,
            policy: Policy::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_http_schemes_are_unrestricted() {
        let policy = Policy {
            allow_hosts: vec!["example.com".into()],
            blocked_hosts: vec![],
        };
        assert!(policy.check("data:text/html,hi").is_ok());
        assert!(policy.check("about:blank").is_ok());
    }

    #[test]
    fn allow_list_restricts_and_blocks_win() {
        let policy = Policy {
            allow_hosts: vec!["example.com".into(), ".trusted.test".into()],
            blocked_hosts: vec!["evil.example.com".into()],
        };
        assert!(policy.check("https://example.com/a").is_ok());
        assert!(policy.check("https://api.trusted.test/").is_ok());
        assert!(policy.check("https://other.com/").is_err());
        assert!(policy.check("https://evil.example.com/").is_err());
    }

    #[test]
    fn empty_allow_list_permits_everything_unblocked() {
        let policy = Policy {
            allow_hosts: vec![],
            blocked_hosts: vec!["blocked.test".into()],
        };
        assert!(policy.check("https://anything.test/").is_ok());
        assert!(policy.check("https://blocked.test/").is_err());
    }
}
