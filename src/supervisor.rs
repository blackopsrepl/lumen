use anyhow::{bail, Context, Result};
use chromiumoxide::cdp::browser_protocol::target::{EventTargetCreated, EventTargetDestroyed};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::Mutex;

use crate::cdp::CdpSession;
use crate::config::Config;
use crate::desktop::{DesktopApp, DesktopBins, DesktopSession};
use crate::pty::PtySession;
use crate::ratatui::RatatuiSession;
use crate::view::ViewHub;
use lumen_ratatui::protocol::{KeyCode, MouseButton, MouseKind};

/// Subdirectory of `data_dir` that holds ephemeral session profiles.
///
/// Lumen owns this tree: it creates one directory per browser instance and
/// removes it when that browser ends. Keeping it in a dedicated subtree means a
/// misconfigured `data_dir` can never make startup reconciliation destructive.
const PROFILE_SUBDIR: &str = "run";

/// Marks a directory as created by Lumen for ephemeral session profiles. The
/// destructive startup reconciliation only runs inside a tree that carries it.
const PROFILE_MARKER: &str = ".lumen-profile-root";
const PROFILE_MARKER_CONTENT: &str = "lumen ephemeral session profiles; safe to clear\n";

/// Advisory lock held for the life of the process, so two instances can never
/// manage — and delete — each other's profiles.
const PROFILE_LOCK: &str = ".lumen-profile.lock";

/// How often the running service reclaims profiles whose purge failed.
const PROFILE_SWEEP_INTERVAL: Duration = Duration::from_secs(300);

static PROFILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Owns every agent session: launch, isolation, discovery, and teardown.
pub struct Supervisor {
    config: Arc<Config>,
    agents: Mutex<HashMap<String, Arc<AgentBrowser>>>,
    /// Held for the process lifetime; dropping it releases the profile-root
    /// lock to the next instance.
    _lock: std::fs::File,
}

/// One isolated browser or desktop session and its managed view.
pub struct AgentBrowser {
    pub name: String,
    pub backend: SessionBackend,
    /// Loopback CDP endpoint for browser sessions; empty for desktop sessions.
    pub cdp_endpoint: String,
    pub view: Arc<ViewHub>,
    /// Ephemeral profile directory owned by this browser instance. It is
    /// removed on shutdown, and reclaimed at startup if this process died
    /// first; it is never reused by another instance.
    profile: PathBuf,
    meta: Mutex<SessionMeta>,
    child: Mutex<Option<Child>>,
}

/// The kind of surface Lumen supervises.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    #[default]
    Browser,
    Quickshell,
    /// A Qt application on a headless Wayland output. Unlike `Quickshell`, the
    /// caller supplies any executable, and the session exposes an accessibility
    /// tree an agent can read and click.
    Qt,
    Ratatui,
    /// A program run in a real pseudoterminal and parsed into a grid. This is
    /// the generic path: the program needs no Lumen support, unlike `Ratatui`.
    Terminal,
}

/// The live backend behind a session.
#[derive(Clone)]
pub enum SessionBackend {
    Browser(Arc<CdpSession>),
    Quickshell(Arc<DesktopSession>),
    Qt(Arc<DesktopSession>),
    Ratatui(Arc<RatatuiSession>),
    Terminal(Arc<PtySession>),
}

impl SessionBackend {
    pub fn kind(&self) -> SessionKind {
        match self {
            Self::Browser(_) => SessionKind::Browser,
            Self::Quickshell(_) => SessionKind::Quickshell,
            Self::Qt(_) => SessionKind::Qt,
            Self::Ratatui(_) => SessionKind::Ratatui,
            Self::Terminal(_) => SessionKind::Terminal,
        }
    }

    pub fn browser(&self) -> Option<Arc<CdpSession>> {
        match self {
            Self::Browser(session) => Some(session.clone()),
            Self::Quickshell(_) | Self::Qt(_) | Self::Ratatui(_) | Self::Terminal(_) => None,
        }
    }

    pub fn desktop(&self) -> Option<Arc<DesktopSession>> {
        match self {
            Self::Browser(_) | Self::Ratatui(_) | Self::Terminal(_) => None,
            Self::Quickshell(session) | Self::Qt(session) => Some(session.clone()),
        }
    }

    pub fn ratatui(&self) -> Option<Arc<RatatuiSession>> {
        match self {
            Self::Browser(_) | Self::Quickshell(_) | Self::Qt(_) | Self::Terminal(_) => None,
            Self::Ratatui(session) => Some(session.clone()),
        }
    }

    /// A terminal-backed session: either a Lumen-native ratatui app or a PTY.
    pub fn terminal(&self) -> Option<TerminalBackend> {
        match self {
            Self::Ratatui(session) => Some(TerminalBackend::Ratatui(session.clone())),
            Self::Terminal(session) => Some(TerminalBackend::Pty(session.clone())),
            Self::Browser(_) | Self::Quickshell(_) | Self::Qt(_) => None,
        }
    }
}

/// A grid-producing backend, so the HTTP layer can treat the two terminal
/// kinds uniformly without caring which one is underneath.
#[derive(Clone)]
pub enum TerminalBackend {
    Ratatui(Arc<RatatuiSession>),
    Pty(Arc<PtySession>),
}

impl TerminalBackend {
    pub fn screen_text(&self) -> String {
        match self {
            Self::Ratatui(session) => session.screen_text(),
            Self::Pty(session) => session.screen_text(),
        }
    }

    pub fn text(&self, text: &str) -> Result<()> {
        match self {
            Self::Ratatui(session) => session.text(text),
            Self::Pty(session) => session.text(text),
        }
    }

    pub fn key(&self, code: KeyCode, mods: u8) -> Result<()> {
        match self {
            Self::Ratatui(session) => session.key(code, mods),
            Self::Pty(session) => session.key(code, mods),
        }
    }

    pub fn mouse(
        &self,
        kind: MouseKind,
        col: u16,
        row: u16,
        button: MouseButton,
        mods: u8,
    ) -> Result<()> {
        match self {
            Self::Ratatui(session) => session.mouse(kind, col, row, button, mods),
            Self::Pty(session) => session.mouse(kind, col, row, button, mods),
        }
    }
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
    kind: SessionKind,
    path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentInfo {
    pub name: String,
    pub kind: SessionKind,
    /// Empty for desktop sessions, which do not expose a CDP endpoint.
    pub cdp_endpoint: String,
    pub path: Option<PathBuf>,
    pub origin: Origin,
    pub owner: Option<String>,
}

#[derive(Debug)]
pub struct SessionConflict(pub String);

impl fmt::Display for SessionConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SessionConflict {}

/// A session name that would be unsafe as a URL segment, a log field, or a
/// directory component.
///
/// Callers get this as a `400`, not a `500`: the request was malformed, the
/// service is fine.
#[derive(Debug)]
pub struct InvalidAgentName(pub String);

impl fmt::Display for InvalidAgentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid agent name '{}' (use [A-Za-z0-9._-], 1-32 chars)",
            self.0
        )
    }
}

impl std::error::Error for InvalidAgentName {}

#[derive(Debug)]
pub struct InvalidQuickshellPath(pub String);

impl fmt::Display for InvalidQuickshellPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidQuickshellPath {}

#[derive(Debug)]
pub struct InvalidRatatuiPath(pub String);

impl fmt::Display for InvalidRatatuiPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidRatatuiPath {}

/// A terminal command that is missing, not absolute, or not executable.
#[derive(Debug)]
pub struct InvalidTerminalCommand(pub String);

impl fmt::Display for InvalidTerminalCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidTerminalCommand {}

/// A Qt command that is missing, not absolute, or not executable.
#[derive(Debug)]
pub struct InvalidQtCommand(pub String);

impl fmt::Display for InvalidQtCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidQtCommand {}

impl AgentBrowser {
    pub async fn info(&self) -> AgentInfo {
        let meta = self.meta.lock().await;
        AgentInfo {
            name: self.name.clone(),
            kind: meta.kind,
            cdp_endpoint: self.cdp_endpoint.clone(),
            path: meta.path.clone(),
            origin: meta.origin,
            owner: meta.owner.clone(),
        }
    }
}

impl Supervisor {
    /// Prepare the profile tree, reclaiming whatever a previous process left.
    ///
    /// Reconciliation happens before the service accepts requests: no browser
    /// can be live yet, so every directory under the profile root is residue
    /// from a crash, an interrupted teardown, or a restart.
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let root = profile_root(&config.data_dir);
        let lock = lock_profile_root(&root)?;
        reconcile_profile_root(&root)?;
        Ok(Self {
            config,
            agents: Mutex::new(HashMap::new()),
            _lock: lock,
        })
    }

    /// Start the background sweeper that reclaims profiles whose purge failed
    /// while the service kept running. Detached: the task lives for the process.
    pub fn spawn_janitor(self: &Arc<Self>) {
        let supervisor = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(PROFILE_SWEEP_INTERVAL);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                supervisor.sweep_orphan_profiles().await;
            }
        });
    }

    /// Return the agent's browser, launching it on first use. Provenance is
    /// left unchanged.
    pub async fn ensure(&self, name: &str) -> Result<Arc<AgentBrowser>> {
        self.ensure_kind_as(name, SessionKind::Browser, None, None)
            .await
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
        self.ensure_kind_as(name, SessionKind::Browser, None, provenance)
            .await
    }

    pub async fn ensure_kind_as(
        &self,
        name: &str,
        kind: SessionKind,
        path: Option<PathBuf>,
        provenance: Option<(Origin, Option<String>)>,
    ) -> Result<Arc<AgentBrowser>> {
        if !is_valid_agent_name(name) {
            return Err(InvalidAgentName(name.to_string()).into());
        }
        let path = match kind {
            SessionKind::Browser if path.is_some() => {
                return Err(SessionConflict("browser sessions do not accept a path".into()).into())
            }
            SessionKind::Browser => None,
            SessionKind::Quickshell => Some(validate_quickshell_path(path)?),
            SessionKind::Qt => Some(validate_qt_command(path)?),
            SessionKind::Ratatui => Some(validate_ratatui_path(path)?),
            SessionKind::Terminal => Some(validate_terminal_command(path)?),
        };

        self.reap_dead().await;
        let mut agents = self.agents.lock().await;
        if let Some(existing) = agents.get(name).cloned() {
            let meta = existing.meta.lock().await;
            if meta.kind != kind || meta.path != path {
                return Err(SessionConflict(format!(
                    "session '{name}' already exists as {:?}",
                    meta.kind
                ))
                .into());
            }
            drop(meta);
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

        // Launching while holding the lock is what makes the sweeper safe: a
        // browser directory is never visible without its map entry.
        let agent = self.launch(name, kind, path.clone()).await?;
        if let Some((origin, owner)) = provenance {
            let mut meta = agent.meta.lock().await;
            meta.origin = origin;
            meta.owner = owner;
        }
        agent.meta.lock().await.path = path;
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
        let agent = {
            let mut agents = self.agents.lock().await;
            let agent = agents.get(name).cloned()?;
            if agent.is_alive().await {
                return Some(agent);
            }
            agents.remove(name);
            agent
        };
        agent.shutdown().await;
        None
    }

    /// Stop and forget a session. Returns whether it existed.
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

    /// Stop every session.
    ///
    /// Service shutdown calls this so no browser outlives the supervisor,
    /// wherever the service runs. Relies on nothing else re-inserting agents
    /// afterwards; the HTTP plane is already draining when it runs.
    pub async fn shutdown_all(&self) {
        let agents: Vec<Arc<AgentBrowser>> = {
            let mut agents = self.agents.lock().await;
            agents.drain().map(|(_, agent)| agent).collect()
        };
        for agent in agents {
            agent.shutdown().await;
        }
    }

    async fn launch(
        &self,
        name: &str,
        kind: SessionKind,
        path: Option<PathBuf>,
    ) -> Result<Arc<AgentBrowser>> {
        let profile = create_profile_dir(&profile_root(&self.config.data_dir), name)?;
        // Until the browser is registered, an error path would leak the profile.
        let mut guard = ProfileGuard::new(profile.clone());

        let viewport = self.config.default_viewport;
        let desktop_bins = DesktopBins {
            sway: &self.config.sway_bin,
            wtype: &self.config.wtype_bin,
            dbus: &self.config.dbus_bin,
            registryd: self.config.at_spi_registryd.as_deref(),
        };
        let (backend, cdp_endpoint, child) = match kind {
            SessionKind::Browser => {
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
                    // Give Chromium its own process group so teardown can signal every
                    // descendant (renderers, GPU, crashpad), not just the parent.
                    .process_group(0)
                    .spawn()
                    .with_context(|| format!("spawning {}", self.config.chrome_bin))?;

                let stderr = child.stderr.take().context("chromium stderr unavailable")?;
                let port = discover_port(stderr).await?;
                let cdp_endpoint = format!("http://127.0.0.1:{port}");

                let session =
                    CdpSession::connect_with_policy(&cdp_endpoint, self.config.policy.clone())
                        .await?;
                session
                    .set_viewport(viewport.width, viewport.height)
                    .await?;
                (SessionBackend::Browser(session), cdp_endpoint, Some(child))
            }
            SessionKind::Quickshell => {
                let path = path.as_deref().expect("validated Quickshell path");
                let app = DesktopApp {
                    program: self.config.quickshell_bin.clone(),
                    args: vec!["--path".into(), path.to_string_lossy().into_owned()],
                };
                // Quickshell publishes no accessibility tree, so the session
                // bus and registry would only add startup work.
                let session = DesktopSession::launch(
                    &desktop_bins,
                    &app,
                    false,
                    &profile,
                    viewport.width,
                    viewport.height,
                )
                .await?;
                (SessionBackend::Quickshell(session), String::new(), None)
            }
            SessionKind::Qt => {
                let command = path
                    .as_deref()
                    .expect("validated Qt command")
                    .to_string_lossy()
                    .to_string();
                let (program, args) = crate::pty::parse_command(&command)?;
                let app = DesktopApp { program, args };
                let session = DesktopSession::launch(
                    &desktop_bins,
                    &app,
                    true,
                    &profile,
                    viewport.width,
                    viewport.height,
                )
                .await?;
                (SessionBackend::Qt(session), String::new(), None)
            }
            SessionKind::Ratatui => {
                let bin = path.as_deref().expect("validated Ratatui path");
                let session = RatatuiSession::launch(
                    bin,
                    &profile,
                    self.config.tui_cols,
                    self.config.tui_rows,
                )
                .await?;
                (SessionBackend::Ratatui(session), String::new(), None)
            }
            SessionKind::Terminal => {
                let command = path
                    .as_deref()
                    .expect("validated terminal command")
                    .to_string_lossy()
                    .to_string();
                let (program, args) = crate::pty::parse_command(&command)?;
                let session = PtySession::launch(
                    &program,
                    &args,
                    &profile,
                    self.config.tui_cols,
                    self.config.tui_rows,
                )
                .await?;
                (SessionBackend::Terminal(session), String::new(), None)
            }
        };
        let view = match &backend {
            SessionBackend::Browser(session) => {
                let view = Arc::new(ViewHub::new(session.clone()));
                spawn_tab_watcher(session.clone(), view.clone());
                view
            }
            SessionBackend::Quickshell(session) => Arc::new(ViewHub::new_desktop(session.clone())),
            SessionBackend::Qt(session) => Arc::new(ViewHub::new_desktop(session.clone())),
            SessionBackend::Ratatui(session) => Arc::new(ViewHub::new_ratatui(session.clone())),
            SessionBackend::Terminal(session) => Arc::new(ViewHub::new_pty(session.clone())),
        };

        tracing::info!(agent = name, ?kind, profile = %profile.display(), "agent session ready");
        let profile = guard.disarm();
        Ok(Arc::new(AgentBrowser {
            name: name.to_string(),
            backend,
            cdp_endpoint,
            view,
            profile,
            meta: Mutex::new(SessionMeta {
                origin: Origin::Manual,
                owner: None,
                kind,
                path,
            }),
            child: Mutex::new(child),
        }))
    }

    async fn reap_dead(&self) {
        let dead: Vec<Arc<AgentBrowser>> = {
            let mut agents = self.agents.lock().await;
            let names: Vec<String> = agents.keys().cloned().collect();
            let mut dead = Vec::new();
            for name in &names {
                if let Some(agent) = agents.get(name) {
                    if !agent.is_alive().await {
                        dead.push(name.clone());
                    }
                }
            }
            dead.into_iter()
                .filter_map(|name| agents.remove(&name))
                .collect()
        };
        for agent in dead {
            agent.shutdown().await;
        }
    }

    /// Reclaim profiles no live browser owns, for purges that failed earlier.
    ///
    /// The live set and the directory listing are read under the agents lock,
    /// which launches also hold for their whole lifetime, so a browser being
    /// created can never be mistaken for residue.
    async fn sweep_orphan_profiles(&self) {
        let stale = {
            let agents = self.agents.lock().await;
            let live: HashSet<PathBuf> =
                agents.values().map(|agent| agent.profile.clone()).collect();
            stale_profiles(&profile_root(&self.config.data_dir), &live)
        };
        for path in stale {
            purge_profile(path).await;
        }
    }
}

impl AgentBrowser {
    pub async fn is_alive(&self) -> bool {
        match &self.backend {
            SessionBackend::Browser(session) => {
                let child_alive = match self.child.lock().await.as_mut() {
                    Some(child) => child.try_wait().is_ok_and(|status| status.is_none()),
                    None => false,
                };
                session.is_alive() && child_alive
            }
            SessionBackend::Quickshell(session) => session.is_alive().await,
            SessionBackend::Qt(session) => session.is_alive().await,
            SessionBackend::Ratatui(session) => session.is_alive().await,
            SessionBackend::Terminal(session) => session.is_alive().await,
        }
    }

    /// Stop this agent's session and reclaim its ephemeral runtime directory.
    pub async fn shutdown(&self) {
        self.view.shutdown().await;
        match &self.backend {
            SessionBackend::Browser(_) => {
                if let Some(child) = self.child.lock().await.as_mut() {
                    terminate_process_group(child).await;
                }
            }
            SessionBackend::Quickshell(session) => session.shutdown().await,
            SessionBackend::Qt(session) => session.shutdown().await,
            SessionBackend::Ratatui(session) => session.shutdown().await,
            SessionBackend::Terminal(session) => session.shutdown().await,
        }
        let _ = purge_profile(self.profile.clone()).await;
    }
}

/// Stop a browser or desktop compositor and everything it spawned.
///
/// `Child::kill` signals only the Chromium parent; its renderer, GPU, and
/// crashpad children can outlive it and keep writing into the profile. Since
/// the browser was spawned with its own process group, signalling the group
/// reaches all of them: SIGTERM first for a clean shutdown, SIGKILL if the
/// group is still alive shortly after.
pub(crate) async fn terminate_process_group(child: &mut Child) {
    let Some(pid) = child.id() else {
        return;
    };
    let pgid = pid as i32;
    // SAFETY: `kill` only touches the process group created for this browser;
    // a negative pid targets the group, and errors are ignored on purpose.
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    for _ in 0..20 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
}

/// Follow tabs the browser opens and closes on its own.
///
/// A `target=_blank` link or a popup creates a page target without any Lumen
/// API call, so without this watcher the viewer keeps screencasting the tab
/// the human just left, their input lands in a hidden tab, and the browser
/// looks frozen. The mirror case matters just as much: an agent driving the
/// browser over CDP can close the managed page itself, which used to leave the
/// session reported alive while the viewer and every command targeted a dead
/// page. Each task ends when the browser connection dies.
fn spawn_tab_watcher(session: Arc<CdpSession>, view: Arc<ViewHub>) {
    spawn_created_watcher(session.clone(), view.clone());
    spawn_destroyed_watcher(session, view);
}

fn spawn_created_watcher(session: Arc<CdpSession>, view: Arc<ViewHub>) {
    tokio::spawn(async move {
        let mut created = match session.browser.event_listener::<EventTargetCreated>().await {
            Ok(created) => created,
            Err(err) => {
                tracing::warn!("browser tab watcher unavailable: {err}");
                return;
            }
        };
        while let Some(event) = created.next().await {
            if event.target_info.r#type != "page" {
                continue;
            }
            let target_id = event.target_info.target_id.inner().clone();
            // A brand-new target is not attachable yet, so it is missing from
            // `browser.pages()` on the first attempts; poll briefly for it.
            for _ in 0..ADOPT_ATTEMPTS {
                if session.target_id().await == target_id {
                    break;
                }
                match session.adopt_opened_page(&target_id).await {
                    Ok(true) => {
                        tracing::info!(target = %target_id, "adopted browser-opened tab");
                        view.rebind().await;
                        break;
                    }
                    Ok(false) => {}
                    Err(err) => tracing::debug!(target = %target_id, "adopt attempt failed: {err}"),
                }
                tokio::time::sleep(ADOPT_RETRY).await;
            }
        }
    });
}

fn spawn_destroyed_watcher(session: Arc<CdpSession>, view: Arc<ViewHub>) {
    tokio::spawn(async move {
        let mut destroyed = match session
            .browser
            .event_listener::<EventTargetDestroyed>()
            .await
        {
            Ok(destroyed) => destroyed,
            Err(err) => {
                tracing::warn!("browser tab destruction watcher unavailable: {err}");
                return;
            }
        };
        while let Some(event) = destroyed.next().await {
            let target_id = event.target_id.inner().clone();
            match session.recover_managed_page(&target_id).await {
                Ok(true) => {
                    tracing::info!(target = %target_id, "managed tab was closed; adopted a replacement");
                    view.rebind().await;
                }
                Ok(false) => {}
                Err(err) => {
                    tracing::warn!(target = %target_id, "recovering the managed tab failed: {err}")
                }
            }
        }
    });
}

/// How long adoption may poll for a browser-opened tab to become attachable.
const ADOPT_ATTEMPTS: usize = 20;
const ADOPT_RETRY: Duration = Duration::from_millis(50);

/// Removes a freshly created profile unless the browser is fully constructed.
///
/// Launch failures (port discovery, CDP connect, viewport) must not leak the
/// directory, and only the successful path disarms this.
struct ProfileGuard(Option<PathBuf>);

impl ProfileGuard {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn disarm(&mut self) -> PathBuf {
        self.0.take().expect("profile guard already disarmed")
    }
}

impl Drop for ProfileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = remove_path(&path);
        }
    }
}

fn profile_root(data_dir: &Path) -> PathBuf {
    data_dir.join(PROFILE_SUBDIR)
}

/// Create or adopt the dedicated profile root.
///
/// Refuses a symlinked path so cleanup can never be redirected outside it, and
/// refuses a non-empty directory that Lumen did not create: the dangerous case
/// is a `data_dir` misconfigured to something like `/`, which would otherwise
/// aim the reconciliation at `/run`. Adoption is allowed only when the
/// directory is empty or holds nothing but profile directories, which is how a
/// tree written by an earlier version (with no marker yet) is taken over.
fn prepare_profile_root(root: &Path) -> Result<()> {
    match std::fs::symlink_metadata(root) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                bail!(
                    "profile root {} is a symlink; refusing to manage it",
                    root.display()
                );
            }
            if !meta.is_dir() {
                bail!("profile root {} is not a directory", root.display());
            }
            if root.join(PROFILE_MARKER).exists() {
                return Ok(());
            }
            let entries = std::fs::read_dir(root)
                .with_context(|| format!("reading profile root {}", root.display()))?;
            let foreign: Vec<String> = entries
                .flatten()
                .filter(|entry| {
                    !(entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                        && looks_like_profile_dir(&entry.file_name().to_string_lossy()))
                })
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            if !foreign.is_empty() {
                bail!(
                    "refusing to manage profile root {}: it holds entries Lumen did not create ({}); \
                     point data_dir at a dedicated directory or clear it",
                    root.display(),
                    foreign.join(", ")
                );
            }
            write_profile_marker(root)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(root)
                .with_context(|| format!("creating profile root {}", root.display()))?;
            write_profile_marker(root)
        }
        Err(err) => Err(err).with_context(|| format!("reading profile root {}", root.display())),
    }
}

fn write_profile_marker(root: &Path) -> Result<()> {
    let marker = root.join(PROFILE_MARKER);
    std::fs::write(&marker, PROFILE_MARKER_CONTENT)
        .with_context(|| format!("writing {}", marker.display()))
}

/// Whether a name matches the generated profile-directory pattern
/// `<session>-<pid>-<counter>-<nanos>`. Session names may themselves contain
/// `-`, so the check anchors on the last three numeric fields.
fn looks_like_profile_dir(name: &str) -> bool {
    let mut parts = name.rsplitn(4, '-');
    let nanos = parts.next();
    let counter = parts.next();
    let pid = parts.next();
    let session = parts.next();
    let numeric = |value: Option<&str>| {
        value.is_some_and(|value| !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()))
    };
    numeric(nanos)
        && numeric(counter)
        && numeric(pid)
        && session.is_some_and(|session| !session.is_empty())
}

/// Take the exclusive profile-root lock, preparing the tree first.
///
/// A second instance sharing `data_dir` fails here instead of deleting the
/// profiles of the first.
fn lock_profile_root(root: &Path) -> Result<std::fs::File> {
    prepare_profile_root(root)?;
    let path = root.join(PROFILE_LOCK);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    // SAFETY: `flock` is called on a live fd we own and only sets an advisory
    // lock; a non-zero return is handled as a normal error.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if locked != 0 {
        bail!(
            "profile root {} is already managed by another running lumen instance",
            root.display()
        );
    }
    Ok(file)
}

/// Create a unique, service-generated profile directory for one session.
///
/// The path never derives from API input, so no session name can escape the
/// root, and the suffix keeps a fresh browser from reusing a profile left
/// behind by a crashed one.
fn create_profile_dir(root: &Path, name: &str) -> Result<PathBuf> {
    prepare_profile_root(root)?;
    for _ in 0..16 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let counter = PROFILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("{name}-{}-{counter}-{nanos}", std::process::id()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err).with_context(|| format!("creating profile {}", dir.display()))
            }
        }
    }
    bail!(
        "could not allocate a unique profile directory under {}",
        root.display()
    )
}

/// Remove a profile path, treating an already-absent path as success.
fn remove_path(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Remove a profile, retrying briefly around surviving child processes.
///
/// Chromium's crashpad, GPU, and renderer processes can outlive the browser
/// process for a moment and keep writing into the profile, which makes a
/// single `remove_dir_all` fail with `Directory not empty`.
fn remove_path_retrying(path: &Path) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..5 {
        match remove_path(path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last = Some(err);
                std::thread::sleep(Duration::from_millis(50 * (attempt + 1)));
            }
        }
    }
    Err(last.expect("at least one removal attempt"))
}

/// Bytes a profile occupies; symlinks are never followed.
fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
            Ok(kind) if kind.is_symlink() => 0,
            Ok(_) => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

/// Remove one profile, reporting the bytes reclaimed.
///
/// Best effort: a failure is logged and either the running sweeper or the next
/// startup reconciliation retries, so teardown never blocks on disk errors.
async fn purge_profile(path: PathBuf) -> Option<u64> {
    let shown = path.clone();
    // Profile removal is unbounded blocking I/O; keep it off the runtime.
    match tokio::task::spawn_blocking(move || {
        let bytes = dir_size(&path);
        remove_path_retrying(&path).map(|()| bytes)
    })
    .await
    {
        Ok(Ok(bytes)) => {
            tracing::info!(profile = %shown.display(), bytes, "removed session profile");
            Some(bytes)
        }
        Ok(Err(err)) => {
            tracing::warn!(profile = %shown.display(), "removing session profile failed: {err}");
            None
        }
        Err(err) => {
            tracing::warn!(profile = %shown.display(), "profile cleanup task failed: {err}");
            None
        }
    }
}

/// Remove every session profile left by a previous process, keeping the root's own
/// marker and lock.
fn reconcile_profile_root(root: &Path) -> Result<()> {
    prepare_profile_root(root)?;
    let entries = std::fs::read_dir(root)
        .with_context(|| format!("reading profile root {}", root.display()))?;
    let mut removed = 0usize;
    let mut bytes = 0u64;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == PROFILE_MARKER || name == PROFILE_LOCK {
            continue;
        }
        let path = entry.path();
        let size = dir_size(&path);
        match remove_path_retrying(&path) {
            Ok(()) => {
                removed += 1;
                bytes += size;
            }
            Err(err) => {
                tracing::warn!(profile = %path.display(), "removing stale profile failed: {err}")
            }
        }
    }
    if removed > 0 {
        tracing::info!(removed, bytes, root = %root.display(), "reconciled stale session profiles");
    }
    Ok(())
}

/// Profile directories that no live browser owns. Files (the root's marker and
/// lock) are never treated as profiles.
fn stale_profiles(root: &Path, live: &HashSet<PathBuf>) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .filter(|path| !live.contains(path))
        .collect()
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

/// Whether a session name is safe to embed in URLs, logs, and generated
/// profile directory names.
pub fn is_valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn validate_quickshell_path(path: Option<PathBuf>) -> Result<PathBuf> {
    let path = path.ok_or_else(|| {
        InvalidQuickshellPath("Quickshell sessions require a path to shell.qml".into())
    })?;
    if !path.is_absolute() {
        return Err(InvalidQuickshellPath("Quickshell path must be absolute".into()).into());
    }
    let metadata = std::fs::metadata(&path).map_err(|err| {
        InvalidQuickshellPath(format!("reading Quickshell path {}: {err}", path.display()))
    })?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(
            InvalidQuickshellPath("Quickshell path must be a file or directory".into()).into(),
        );
    }
    Ok(std::fs::canonicalize(&path).map_err(|err| {
        InvalidQuickshellPath(format!(
            "resolving Quickshell path {}: {err}",
            path.display()
        ))
    })?)
}

/// Validate a ratatui app path and return it canonicalized.
///
/// The path names an executable that links `lumen-ratatui`; like a Quickshell
/// path, this is a file-existence check, not a sandbox. Lumen executes whatever
/// binary it is pointed at.
fn validate_ratatui_path(path: Option<PathBuf>) -> Result<PathBuf> {
    let path = path.ok_or_else(|| {
        InvalidRatatuiPath("Ratatui sessions require a path to the app binary".into())
    })?;
    if !path.is_absolute() {
        return Err(InvalidRatatuiPath("Ratatui path must be absolute".into()).into());
    }
    let metadata = std::fs::metadata(&path).map_err(|err| {
        InvalidRatatuiPath(format!("reading Ratatui path {}: {err}", path.display()))
    })?;
    if !metadata.is_file() {
        return Err(InvalidRatatuiPath("Ratatui path must be an executable file".into()).into());
    }
    if is_executable(&metadata) {
        return Ok(std::fs::canonicalize(&path).map_err(|err| {
            InvalidRatatuiPath(format!("resolving Ratatui path {}: {err}", path.display()))
        })?);
    }
    Err(InvalidRatatuiPath(format!("Ratatui path {} is not executable", path.display())).into())
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    true
}

/// Validate a terminal command and return it canonicalized.
///
/// A terminal session runs an arbitrary program, so this is the strongest of
/// the path checks: the program must be absolute and executable. Arguments are
/// allowed after the program, which are passed through verbatim. Like the
/// Quickshell and ratatui checks, this rejects a typo or a relative path; it is
/// not a sandbox, and the program runs with the service's privileges.
fn validate_terminal_command(path: Option<PathBuf>) -> Result<PathBuf> {
    validate_command(path, "Terminal sessions require a command to run")
        .map_err(|message| InvalidTerminalCommand(message).into())
}

/// Validate a Qt application command and return it canonicalized.
///
/// A Qt session runs an arbitrary GUI program, so the check is the same as a
/// terminal command's: absolute and executable, with arguments passed through.
/// It is a typo check, not a sandbox.
fn validate_qt_command(path: Option<PathBuf>) -> Result<PathBuf> {
    validate_command(path, "Qt sessions require a command to run")
        .map_err(|message| InvalidQtCommand(message).into())
}

/// Parse and canonicalize an absolute executable command.
///
/// The program half is resolved to a canonical path; the argument text is
/// preserved verbatim. `missing` is the message used when no command was given.
fn validate_command(path: Option<PathBuf>, missing: &str) -> std::result::Result<PathBuf, String> {
    let path = path.ok_or_else(|| missing.to_string())?;
    let text = path.to_string_lossy().to_string();
    let (program, _args) = crate::pty::parse_command(&text)
        .map_err(|err| format!("invalid command {text:?}: {err}"))?;
    if !program.starts_with('/') {
        return Err(format!("program {program:?} must be an absolute path"));
    }
    let program_path = PathBuf::from(&program);
    let metadata = std::fs::metadata(&program_path).map_err(|err| {
        format!(
            "reading program {program}: {err} (the program must exist in the service's \
             filesystem; in a container it must be mounted, see LUMEN_PROJECTS_ROOT)"
        )
    })?;
    if !metadata.is_file() || !is_executable(&metadata) {
        return Err(format!("program {program} is not an executable file"));
    }
    // The command is preserved verbatim (program plus arguments); only the
    // program half is resolved, so argument text is never rewritten.
    let canonical = std::fs::canonicalize(&program_path)
        .map_err(|err| format!("resolving program {program}: {err}"))?;
    if let Some(rest) = text.strip_prefix(&program) {
        Ok(PathBuf::from(format!("{}{}", canonical.display(), rest)))
    } else {
        Ok(canonical)
    }
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

    #[test]
    fn creates_a_unique_profile_per_call() {
        let base = temp_root("unique");
        let root = profile_root(&base);
        let first = create_profile_dir(&root, "alice").expect("first profile");
        let second = create_profile_dir(&root, "alice").expect("second profile");
        assert_ne!(first, second);
        assert!(first.starts_with(&root));
        assert!(first
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .starts_with("alice-"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn reconciliation_clears_only_the_profile_root() {
        let base = temp_root("reconcile");
        let root = profile_root(&base);
        std::fs::create_dir_all(root.join("alice-1-1-1")).expect("leftover");
        std::fs::write(root.join(PROFILE_MARKER), "x").expect("marker");
        std::fs::create_dir_all(base.join("keep")).expect("keep");
        reconcile_profile_root(&root).expect("reconcile");
        assert!(!root.join("alice-1-1-1").exists());
        assert!(root.join(PROFILE_MARKER).exists());
        assert!(base.join("keep").exists());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn refuses_a_non_empty_root_it_did_not_create() {
        let base = temp_root("foreign");
        let root = profile_root(&base);
        std::fs::create_dir_all(root.join("systemd")).expect("foreign entry");
        assert!(prepare_profile_root(&root).is_err());
        assert!(root.join("systemd").exists());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn adopts_an_empty_root_and_a_root_of_old_profiles() {
        let base = temp_root("adopt");
        let root = profile_root(&base);
        std::fs::create_dir_all(&root).expect("root");
        prepare_profile_root(&root).expect("adopt empty");
        assert!(root.join(PROFILE_MARKER).exists());

        let upgraded = temp_root("upgrade");
        let upgraded_root = profile_root(&upgraded);
        std::fs::create_dir_all(upgraded_root.join("alice-7-2-1789482902823221842"))
            .expect("old profile");
        prepare_profile_root(&upgraded_root).expect("adopt old profiles");
        assert!(upgraded_root.join(PROFILE_MARKER).exists());

        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_dir_all(&upgraded).ok();
    }

    #[test]
    fn refuses_a_second_instance_on_the_same_root() {
        let base = temp_root("lock");
        let root = profile_root(&base);
        let first = lock_profile_root(&root).expect("first instance");
        assert!(lock_profile_root(&root).is_err());
        drop(first);
        assert!(lock_profile_root(&root).is_ok());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn refuses_a_symlinked_profile_root() {
        let base = temp_root("symlink");
        let target = base.join("target");
        std::fs::create_dir_all(&target).expect("target");
        std::os::unix::fs::symlink(&target, profile_root(&base)).expect("symlink");
        assert!(reconcile_profile_root(&profile_root(&base)).is_err());
        assert!(target.exists());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn sweeps_only_profiles_without_a_live_browser() {
        let base = temp_root("sweep");
        let root = profile_root(&base);
        std::fs::create_dir_all(root.join("live")).expect("live");
        std::fs::create_dir_all(root.join("stale")).expect("stale");
        let live: HashSet<PathBuf> = [root.join("live")].into_iter().collect();
        assert_eq!(stale_profiles(&root, &live), vec![root.join("stale")]);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn removing_an_absent_profile_is_not_an_error() {
        let base = temp_root("absent");
        assert!(remove_path(&base.join("gone")).is_ok());
        assert!(remove_path_retrying(&base.join("gone")).is_ok());
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn qt_command_requires_an_absolute_executable() {
        assert!(validate_qt_command(None).is_err());
        assert!(validate_qt_command(Some(PathBuf::from("relative/app"))).is_err());
        assert!(validate_qt_command(Some(PathBuf::from("/nonexistent/app"))).is_err());
        assert!(validate_qt_command(Some(PathBuf::from("/tmp"))).is_err());
        assert!(validate_qt_command(Some(PathBuf::from("/bin/sh"))).is_ok());
    }

    #[test]
    fn command_arguments_are_preserved_verbatim() {
        let resolved = validate_qt_command(Some(PathBuf::from("/bin/sh -c 'echo hi'")))
            .expect("absolute executable with arguments");
        let text = resolved.to_string_lossy();
        assert!(
            text.contains(" -c 'echo hi'"),
            "arguments were rewritten: {text}"
        );
    }

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lumen-{label}-{nanos}"));
        std::fs::create_dir_all(&path).expect("temp root");
        path
    }
}
