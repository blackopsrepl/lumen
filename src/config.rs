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
    /// Root for ephemeral per-session browser profiles. Lumen owns
    /// `<data_dir>/run`: it creates one profile per browser instance and
    /// removes it when that browser ends, and clears the tree at startup.
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
///
/// [`Policy::check`] is the single predicate behind both enforcement points:
/// Lumen's HTTP preflight and the per-tab CDP interception in
/// [`crate::cdp::install_navigation_policy`]. The interception covers every
/// tab Lumen mediates, so navigations there are checked no matter which CDP
/// session issues them. Tabs created without Lumen's API are not intercepted
/// until Lumen's API touches them; this policy is not a browser-wide sandbox.
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
    let host = host.trim_end_matches('.');
    let entry = entry.trim().trim_end_matches('.');
    if let Some(suffix) = entry.strip_prefix('.') {
        host.eq_ignore_ascii_case(suffix)
            || host
                .to_ascii_lowercase()
                .ends_with(&format!(".{}", suffix.to_ascii_lowercase()))
    } else {
        host.eq_ignore_ascii_case(entry)
    }
}

impl Policy {
    pub fn is_restricted(&self) -> bool {
        !self.allow_hosts.is_empty() || !self.blocked_hosts.is_empty()
    }

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
        let mut config = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let config: Config =
                    toml::from_str(&text).with_context(|| format!("invalid config at {path}"))?;
                config
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(err) => return Err(err).with_context(|| format!("failed to read {path}")),
        };

        if let Ok(port) = std::env::var("LUMEN_PORT") {
            config.port = port
                .parse()
                .with_context(|| format!("invalid LUMEN_PORT '{port}'"))?;
        }
        if let Ok(chrome_bin) = std::env::var("LUMEN_CHROME") {
            config.chrome_bin = chrome_bin;
        }

        Ok(config)
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

    #[test]
    fn restriction_is_detected_from_either_host_list() {
        assert!(!Policy::default().is_restricted());
        assert!(Policy {
            allow_hosts: vec!["example.com".into()],
            blocked_hosts: vec![],
        }
        .is_restricted());
        assert!(Policy {
            allow_hosts: vec![],
            blocked_hosts: vec!["evil.test".into()],
        }
        .is_restricted());
    }

    #[test]
    fn host_matching_ignores_dns_case_and_trailing_dots() {
        let policy = Policy {
            allow_hosts: vec!["Example.COM.".into()],
            blocked_hosts: vec![".Evil.TEST.".into()],
        };
        assert!(policy.check("https://example.com./").is_ok());
        assert!(policy.check("https://evil.test./").is_err());
        assert!(policy.check("https://api.eViL.TeSt/").is_err());
    }
}
