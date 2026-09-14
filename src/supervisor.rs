use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::Mutex;

use crate::cdp::CdpSession;
use crate::config::Config;
use crate::view::ViewHub;

/// Owns every agent browser: launch, isolation, discovery, and teardown.
pub struct Supervisor {
    config: Arc<Config>,
    agents: Mutex<HashMap<String, Arc<AgentBrowser>>>,
}

/// One isolated Chromium and the CDP session bound to its managed page.
pub struct AgentBrowser {
    pub name: String,
    /// Loopback HTTP endpoint (`http://127.0.0.1:<port>`) that both the agent's
    /// `playwright-cli` and Lumen's own CDP session attach to.
    pub cdp_endpoint: String,
    pub session: Arc<CdpSession>,
    pub view: Arc<ViewHub>,
    meta: Mutex<SessionMeta>,
    child: Mutex<Child>,
}

/// Where a session came from, so the human can tell an attended browser from a
/// stray one: `Agent` means an agent registered it (and will read its feedback),
/// `Manual` means a human created it from the viewer with nobody attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Manual,
    Agent,
}

#[derive(Debug, Clone)]
struct SessionMeta {
    origin: Origin,
    owner: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub name: String,
    pub cdp_endpoint: String,
    pub origin: Origin,
    pub owner: Option<String>,
}

impl AgentBrowser {
    pub async fn info(&self) -> AgentInfo {
        let meta = self.meta.lock().await;
        AgentInfo {
            name: self.name.clone(),
            cdp_endpoint: self.cdp_endpoint.clone(),
            origin: meta.origin,
            owner: meta.owner.clone(),
        }
    }
}

impl Supervisor {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            agents: Mutex::new(HashMap::new()),
        }
    }

    /// Return the agent's browser, launching it on first use. Provenance is
    /// left unchanged.
    pub async fn ensure(&self, name: &str) -> Result<Arc<AgentBrowser>> {
        self.ensure_as(name, None).await
    }

    /// Return the agent's browser, recording who registered it.
    ///
    /// A new session takes the given provenance (defaulting to `Manual`). An
    /// existing one is only upgraded toward `Agent`, so an agent that later
    /// adopts a human-created session claims it without the reverse ever
    /// happening.
    pub async fn ensure_as(
        &self,
        name: &str,
        provenance: Option<(Origin, Option<String>)>,
    ) -> Result<Arc<AgentBrowser>> {
        if !is_valid_agent_name(name) {
            bail!("invalid agent name '{name}' (use [A-Za-z0-9._-], 1-32 chars)");
        }

        self.reap_dead().await;
        let mut agents = self.agents.lock().await;
        if let Some(existing) = agents.get(name).cloned() {
            if let Some((origin, owner)) = provenance {
                if origin == Origin::Agent {
                    let mut meta = existing.meta.lock().await;
                    meta.origin = Origin::Agent;
                    if owner.is_some() {
                        meta.owner = owner;
                    }
                }
            }
            return Ok(existing);
        }
        if agents.len() >= self.config.max_agents {
            bail!("max agents ({}) reached", self.config.max_agents);
        }

        let agent = self.launch(name).await?;
        if let Some((origin, owner)) = provenance {
            let mut meta = agent.meta.lock().await;
            meta.origin = origin;
            meta.owner = owner;
        }
        agents.insert(name.to_string(), agent.clone());
        Ok(agent)
    }

    pub async fn list(&self) -> Vec<AgentInfo> {
        self.reap_dead().await;
        let agents: Vec<Arc<AgentBrowser>> = self.agents.lock().await.values().cloned().collect();
        let mut out = Vec::with_capacity(agents.len());
        for agent in agents {
            out.push(agent.info().await);
        }
        out
    }

    /// Return a running session without creating one.
    pub async fn existing(&self, name: &str) -> Option<Arc<AgentBrowser>> {
        let mut agents = self.agents.lock().await;
        let agent = agents.get(name).cloned()?;
        if agent.is_alive().await {
            return Some(agent);
        }
        agents.remove(name);
        agent.shutdown().await;
        None
    }

    /// Stop and forget a session's browser. Returns whether it existed.
    pub async fn remove(&self, name: &str) -> bool {
        let agent = self.agents.lock().await.remove(name);
        match agent {
            Some(agent) => {
                agent.shutdown().await;
                true
            }
            None => false,
        }
    }

    async fn launch(&self, name: &str) -> Result<Arc<AgentBrowser>> {
        let profile = self.config.data_dir.join(name);
        std::fs::create_dir_all(&profile)
            .with_context(|| format!("creating profile dir {}", profile.display()))?;

        let viewport = self.config.default_viewport;
        let mut child = Command::new(&self.config.chrome_bin)
            .arg("--remote-debugging-port=0")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!(
                "--window-size={},{}",
                viewport.width, viewport.height
            ))
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-dev-shm-usage",
                "--disable-background-networking",
                "--no-sandbox",
                "--disable-setuid-sandbox",
                "--headless=new",
                // A single explicit page; without it Chromium opens a new-tab
                // page alongside about:blank and the drivers disagree on which
                // tab is "the" browser.
                "about:blank",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {}", self.config.chrome_bin))?;

        let stderr = child.stderr.take().context("chromium stderr unavailable")?;
        let port = discover_port(stderr).await?;
        let cdp_endpoint = format!("http://127.0.0.1:{port}");

        let session =
            CdpSession::connect_with_policy(&cdp_endpoint, self.config.policy.clone()).await?;
        session
            .set_viewport(viewport.width, viewport.height)
            .await?;
        let view = Arc::new(ViewHub::new(session.clone()));

        tracing::info!(agent = name, %cdp_endpoint, "agent browser ready");
        Ok(Arc::new(AgentBrowser {
            name: name.to_string(),
            cdp_endpoint,
            session,
            view,
            meta: Mutex::new(SessionMeta {
                origin: Origin::Manual,
                owner: None,
            }),
            child: Mutex::new(child),
        }))
    }

    async fn reap_dead(&self) {
        let mut agents = self.agents.lock().await;
        let names: Vec<String> = agents.keys().cloned().collect();
        let mut dead = Vec::new();
        for name in names {
            if let Some(agent) = agents.get(&name) {
                if !agent.is_alive().await {
                    dead.push(name);
                }
            }
        }
        for name in dead {
            if let Some(agent) = agents.remove(&name) {
                agent.shutdown().await;
            }
        }
    }
}

impl AgentBrowser {
    pub async fn is_alive(&self) -> bool {
        if !self.session.is_alive() {
            return false;
        }
        matches!(self.child.lock().await.try_wait(), Ok(None))
    }

    /// Stop this agent's Chromium.
    pub async fn shutdown(&self) {
        let _ = self.child.lock().await.kill().await;
    }
}

/// Read Chromium's stderr until it announces its DevTools endpoint, then keep
/// draining so the pipe never blocks the browser.
async fn discover_port(stderr: ChildStderr) -> Result<u16> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tx = Some(tx);
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(port) = parse_devtools_port(&line) {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(port);
                }
            }
        }
    });

    match tokio::time::timeout(Duration::from_secs(15), rx).await {
        Ok(Ok(port)) => Ok(port),
        Ok(Err(_)) => bail!("chromium exited before its CDP endpoint was ready"),
        Err(_) => bail!("timed out waiting for chromium's CDP endpoint"),
    }
}

fn parse_devtools_port(line: &str) -> Option<u16> {
    let rest = line.split("DevTools listening on ws://127.0.0.1:").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Whether a session name is safe to use as a profile directory and URL path.
pub fn is_valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_devtools_port() {
        let line =
            "[123:456:INFO:CONSOLE] DevTools listening on ws://127.0.0.1:45123/devtools/browser/x";
        assert_eq!(parse_devtools_port(line), Some(45123));
        assert_eq!(parse_devtools_port("nothing here"), None);
    }

    #[test]
    fn rejects_unsafe_names() {
        assert!(is_valid_agent_name("alice"));
        assert!(is_valid_agent_name("agent.1_x-y"));
        assert!(!is_valid_agent_name(""));
        assert!(!is_valid_agent_name("has space"));
        assert!(!is_valid_agent_name("slash/name"));
        assert!(!is_valid_agent_name("."));
        assert!(!is_valid_agent_name(".."));
        assert!(!is_valid_agent_name(&"a".repeat(33)));
    }
}
