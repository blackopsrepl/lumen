use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::Mutex;

use crate::cdp::CdpSession;
use crate::config::Config;

/// Owns every agent browser: launch, isolation, discovery, and teardown.
pub struct Supervisor {
    config: Arc<Config>,
    agents: Mutex<HashMap<String, Arc<AgentBrowser>>>,
}

/// One isolated Chromium and the CDP session bound to its shared page.
pub struct AgentBrowser {
    pub name: String,
    /// Loopback HTTP endpoint (`http://127.0.0.1:<port>`) that both the agent's
    /// `playwright-cli` and Lumen's own CDP session attach to.
    pub cdp_endpoint: String,
    pub session: Arc<CdpSession>,
    child: Mutex<Child>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub name: String,
    pub cdp_endpoint: String,
}

impl Supervisor {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            agents: Mutex::new(HashMap::new()),
        }
    }

    /// Return the agent's browser, launching it on first use.
    pub async fn ensure(&self, name: &str) -> Result<Arc<AgentBrowser>> {
        if !valid_name(name) {
            bail!("invalid agent name '{name}' (use [A-Za-z0-9._-], 1-32 chars)");
        }

        let mut agents = self.agents.lock().await;
        if let Some(existing) = agents.get(name).cloned() {
            return Ok(existing);
        }
        if agents.len() >= self.config.max_agents {
            bail!("max agents ({}) reached", self.config.max_agents);
        }

        let agent = self.launch(name).await?;
        agents.insert(name.to_string(), agent.clone());
        Ok(agent)
    }

    pub async fn list(&self) -> Vec<AgentInfo> {
        self.agents
            .lock()
            .await
            .values()
            .map(|agent| AgentInfo {
                name: agent.name.clone(),
                cdp_endpoint: agent.cdp_endpoint.clone(),
            })
            .collect()
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

        let session = CdpSession::connect(&cdp_endpoint).await?;
        session
            .set_viewport(viewport.width, viewport.height)
            .await?;

        tracing::info!(agent = name, %cdp_endpoint, "agent browser ready");
        Ok(Arc::new(AgentBrowser {
            name: name.to_string(),
            cdp_endpoint,
            session,
            child: Mutex::new(child),
        }))
    }
}

impl AgentBrowser {
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

fn valid_name(name: &str) -> bool {
    !name.is_empty()
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
        assert!(valid_name("alice"));
        assert!(valid_name("agent.1_x-y"));
        assert!(!valid_name(""));
        assert!(!valid_name("has space"));
        assert!(!valid_name("slash/name"));
        assert!(!valid_name(&"a".repeat(33)));
    }
}
