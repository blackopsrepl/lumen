use crate::accessibility;
use crate::cdp::{CdpSession, NavigationBlocked, OnlyManagedTab, TabInfo};
use crate::config::{Config, Viewport};
use crate::desktop::MouseAction;
use crate::feedback::{AuditEntry, Feedback, FeedbackStore};
use crate::supervisor::{
    is_valid_agent_name, AgentBrowser, AgentInfo, InvalidAgentName, InvalidQtCommand,
    InvalidQuickshellPath, InvalidRatatuiPath, InvalidTerminalCommand, Origin, SessionBackend,
    SessionConflict, SessionKind, Supervisor, TerminalBackend,
};
use crate::view::{Control, ViewHub};
use anyhow::Context as _;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::input::{DispatchMouseEventType, MouseButton};
use futures::{SinkExt, StreamExt};
use lumen_ratatui::protocol::{KeyCode, MouseButton as RatatuiMouseButton, MouseKind};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

#[derive(RustEmbed)]
#[folder = "ui/"]
struct UiAssets;

/// Largest request body the router accepts. The only large body is a feedback
/// screenshot, sized here for a base64 PNG of the largest viewport plus JSON
/// and base64 overhead.
const MAX_FEEDBACK_BODY: usize = 8 * 1024 * 1024;
/// Largest decoded screenshot, so a note cannot bloat the feedback database.
const MAX_SCREENSHOT_BYTES: usize = 4 * 1024 * 1024;
/// The PNG magic bytes; the viewer only ever captures `image/png`.
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
/// Response header naming a tree whose application published no named object,
/// so a caller can tell "nothing addressable" from a short-but-real tree.
const TREE_WARNING_HEADER: header::HeaderName =
    header::HeaderName::from_static("x-lumen-tree-warning");

/// Shared control-plane state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub supervisor: Arc<Supervisor>,
    pub feedback: Arc<FeedbackStore>,
    /// Set once shutdown has begun: new control-plane calls are refused and
    /// every open viewer is told to close, so browser teardown is not racing
    /// requests that are still creating browsers.
    draining: Arc<std::sync::atomic::AtomicBool>,
    shutdown: broadcast::Sender<()>,
    /// Resolves once draining has begun, so the shutdown path can time the
    /// drain without ever bounding the server's ordinary lifetime.
    drain_started: Arc<tokio::sync::Notify>,
}

impl AppState {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let feedback = Arc::new(FeedbackStore::open(
            &config.feedback_db,
            config.audit_retain,
        )?);
        let config = Arc::new(config);
        let supervisor = Arc::new(Supervisor::new(config.clone(), feedback.clone())?);
        supervisor.spawn_janitor();
        let (shutdown, _) = broadcast::channel(1);
        Ok(Self {
            supervisor,
            feedback,
            config,
            draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shutdown,
            drain_started: Arc::new(tokio::sync::Notify::new()),
        })
    }

    /// Refuse new control-plane work and release every viewer.
    pub fn begin_draining(&self) {
        self.draining
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.shutdown.send(());
        self.drain_started.notify_one();
    }

    /// Wait until draining has begun.
    pub async fn drain_started(&self) {
        self.drain_started.notified().await;
    }

    fn is_draining(&self) -> bool {
        self.draining.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Build the HTTP control/view plane.
pub fn router(state: AppState) -> Router {
    let drain_state = state.clone();
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/style.css", get(style_css))
        .route("/fonts/{*path}", get(font))
        .route("/healthz", get(healthz))
        .route("/v1/sessions", get(list_sessions).post(create_session))
        .route(
            "/v1/sessions/{name}",
            get(get_session).delete(delete_session),
        )
        .route("/v1/sessions/{name}/navigate", post(navigate))
        .route(
            "/v1/sessions/{name}/viewport",
            put(set_viewport).delete(reset_viewport),
        )
        .route("/v1/sessions/{name}/stream", get(stream))
        .route("/v1/sessions/{name}/tabs", get(list_tabs).post(open_tab))
        .route(
            "/v1/sessions/{name}/tabs/{index}/activate",
            post(activate_tab),
        )
        .route("/v1/sessions/{name}/tabs/{index}", delete(close_tab))
        .route("/v1/sessions/{name}/screenshot", post(screenshot))
        .route("/v1/sessions/{name}/screen", get(screen))
        .route("/v1/sessions/{name}/accessibility", get(accessibility))
        .route(
            "/v1/sessions/{name}/accessibility/click",
            post(accessibility_click),
        )
        .route(
            "/v1/sessions/{name}/accessibility/type",
            post(accessibility_type),
        )
        .route("/v1/sessions/{name}/cdp", post(raw_cdp))
        .route("/v1/audit", get(list_audit))
        .route("/v1/sessions/{name}/visibility", put(set_visibility))
        .route("/v1/sessions/{name}/page-scale", put(set_page_scale))
        .route(
            "/v1/sessions/{name}/feedback",
            get(list_feedback).post(add_feedback),
        )
        .route(
            "/v1/sessions/{name}/feedback/consume",
            post(consume_feedback),
        )
        .route(
            "/v1/sessions/{name}/feedback/{id}/screenshot",
            get(feedback_screenshot),
        )
        .route("/v1/sessions/{name}/feedback/{id}/ack", post(ack_feedback))
        .route(
            "/v1/sessions/{name}/feedback/ack-all",
            post(ack_all_feedback),
        )
        .layer(DefaultBodyLimit::max(MAX_FEEDBACK_BODY))
        .layer(middleware::from_fn(loopback_guard))
        .layer(middleware::from_fn_with_state(drain_state, draining_guard))
        .with_state(state)
}

/// Refuse control-plane work once shutdown has started, so teardown is not
/// racing requests that would create or drive browsers. Health and the viewer
/// assets stay reachable so the shutdown is observable.
async fn draining_guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if state.is_draining() && request.uri().path().starts_with("/v1/") {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "lumen is shutting down" })),
        )
            .into_response();
    }
    next.run(request).await
}

/// Whether a `Host` header value names the loopback interface.
///
/// Lumen deliberately has no authentication, but it must not be drivable by a
/// page the browser is showing: under host networking any page can reach
/// `127.0.0.1:<port>`. Checking `Host` blocks DNS rebinding, where a hostile
/// name resolves to loopback while still being sent as the `Host`.
fn host_is_loopback(host: &str) -> bool {
    let bare = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else if let Some((name, port)) = host.rsplit_once(':') {
        if port.chars().all(|c| c.is_ascii_digit()) {
            name
        } else {
            host
        }
    } else {
        host
    };
    bare.eq_ignore_ascii_case("localhost")
        || bare
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

/// Whether an `Origin` header value is a loopback origin.
fn origin_is_loopback(origin: &str) -> bool {
    url::Url::parse(origin)
        .ok()
        .and_then(|parsed| parsed.host_str().map(host_is_loopback))
        .unwrap_or(false)
}

/// Reject control/view requests that did not come from the loopback interface.
///
/// This is a boundary against foreign origins, not authentication: any local
/// process may call Lumen, by design. It covers both ordinary requests
/// (`Host`) and the WebSocket upgrades a browser opens (`Origin`).
async fn loopback_guard(request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !host_is_loopback(host) {
        return forbidden_origin(format!("host '{host}' is not loopback"));
    }
    if let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        if !origin_is_loopback(origin) {
            return forbidden_origin(format!("origin '{origin}' is not loopback"));
        }
    }
    next.run(request).await
}

fn forbidden_origin(reason: String) -> Response {
    tracing::warn!("rejected a non-loopback request: {reason}");
    (StatusCode::FORBIDDEN, Json(json!({ "error": reason }))).into_response()
}

fn asset(path: &str, content_type: &str) -> Response {
    match UiAssets::get(path) {
        Some(file) => (
            [(header::CONTENT_TYPE, content_type)],
            file.data.into_owned(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn index() -> Response {
    asset("index.html", "text/html; charset=utf-8")
}

async fn app_js() -> Response {
    asset("app.js", "text/javascript; charset=utf-8")
}

async fn style_css() -> Response {
    asset("style.css", "text/css; charset=utf-8")
}

/// Serve the vendored woff2 files under ui/fonts/. Only plain file names are
/// accepted — no slashes, so the path can never escape the fonts directory.
async fn font(Path(path): Path<String>) -> Response {
    let plausible = path.ends_with(".woff2")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !plausible {
        return StatusCode::NOT_FOUND.into_response();
    }
    asset(&format!("fonts/{path}"), "font/woff2")
}

async fn healthz(State(state): State<AppState>) -> Response {
    if state.is_draining() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "draining" })),
        )
            .into_response();
    }
    Json(json!({ "status": "ok" })).into_response()
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<AgentInfo>> {
    Json(state.supervisor.list().await)
}

fn browser_session(agent: &AgentBrowser) -> Result<Arc<CdpSession>, ApiError> {
    agent
        .backend
        .browser()
        .ok_or_else(|| ApiError::conflict("operation is only available for browser sessions"))
}

fn terminal_session(agent: &AgentBrowser) -> Result<TerminalBackend, ApiError> {
    agent
        .backend
        .terminal()
        .ok_or_else(|| ApiError::conflict("operation is only available for terminal sessions"))
}

async fn view_session(supervisor: &Supervisor, name: &str) -> Result<Arc<AgentBrowser>, ApiError> {
    match supervisor.existing(name).await {
        Some(agent) => Ok(agent),
        None => Ok(supervisor.ensure(name).await?),
    }
}

#[derive(Deserialize)]
struct CreateSession {
    name: String,
    #[serde(default)]
    kind: SessionKind,
    /// Quickshell accepts either a shell.qml file or its containing directory.
    #[serde(default)]
    path: Option<std::path::PathBuf>,
    /// Who is registering the session. Omitted means a human created it from
    /// the viewer; agents send `agent` (optionally with an owner label).
    #[serde(default)]
    origin: Option<Origin>,
    #[serde(default)]
    owner: Option<String>,
}

async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSession>,
) -> Result<Json<AgentInfo>, ApiError> {
    if !is_valid_agent_name(&body.name) {
        return Err(ApiError::bad_request(
            "invalid agent name (use [A-Za-z0-9._-], 1-32 chars)",
        ));
    }
    if body.kind == SessionKind::Browser && body.path.is_some() {
        return Err(ApiError::bad_request(
            "browser sessions do not accept a path",
        ));
    }
    if body.kind == SessionKind::Quickshell && body.path.is_none() {
        return Err(ApiError::bad_request(
            "Quickshell sessions require a path to shell.qml",
        ));
    }
    if body.kind == SessionKind::Ratatui && body.path.is_none() {
        return Err(ApiError::bad_request(
            "Ratatui sessions require a path to the app binary",
        ));
    }
    if body.kind == SessionKind::Terminal && body.path.is_none() {
        return Err(ApiError::bad_request(
            "Terminal sessions require a command to run",
        ));
    }
    if body.kind == SessionKind::Qt && body.path.is_none() {
        return Err(ApiError::bad_request(
            "Qt sessions require a command to run",
        ));
    }
    let origin = body.origin.unwrap_or(Origin::Manual);
    let owner = body.owner.filter(|value| !value.trim().is_empty());
    let agent = state
        .supervisor
        .ensure_kind_as(&body.name, body.kind, body.path, Some((origin, owner)))
        .await?;
    Ok(Json(agent.info().await))
}

async fn get_session(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<AgentInfo>, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    let agent = state
        .supervisor
        .existing(&name)
        .await
        .ok_or_else(|| ApiError::not_found(format!("session '{name}' not found")))?;
    Ok(Json(agent.info().await))
}

async fn delete_session(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    if !state.supervisor.remove(&name).await {
        return Err(ApiError::not_found(format!("session '{name}' not found")));
    }
    if let Err(err) = state
        .feedback
        .record(&name, "delete", "session stopped; profile purged")
        .await
    {
        tracing::warn!("audit write failed: {err}");
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct NavigateBody {
    url: String,
}

async fn navigate(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<NavigateBody>,
) -> Result<StatusCode, ApiError> {
    state
        .config
        .policy
        .check(&body.url)
        .map_err(|err| ApiError::forbidden(err.to_string()))?;
    let agent = state.supervisor.ensure(&name).await?;
    let session = browser_session(&agent)?;
    if let Err(err) = session.goto(&body.url).await {
        if err.downcast_ref::<NavigationBlocked>().is_some() {
            return Err(ApiError::forbidden(err.to_string()));
        }
        return Err(err.into());
    }
    if let Err(err) = state.feedback.record(&name, "navigate", &body.url).await {
        tracing::warn!("audit write failed: {err}");
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ViewportBody {
    width: u32,
    height: u32,
}

async fn set_viewport(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ViewportBody>,
) -> Result<StatusCode, ApiError> {
    if body.width == 0 || body.height == 0 {
        return Err(ApiError::bad_request("width and height must be positive"));
    }
    let agent = state.supervisor.ensure(&name).await?;
    browser_session(&agent)?
        .set_viewport(body.width, body.height)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_viewport(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let agent = state.supervisor.ensure(&name).await?;
    let Viewport { width, height } = state.config.default_viewport;
    browser_session(&agent)?.set_viewport(width, height).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_tabs(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<TabInfo>>, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    let agent = state
        .supervisor
        .existing(&name)
        .await
        .ok_or_else(|| ApiError::not_found(format!("session '{name}' not found")))?;
    Ok(Json(browser_session(&agent)?.tabs().await?))
}

#[derive(Deserialize)]
struct OpenTabBody {
    #[serde(default = "about_blank")]
    url: String,
}

fn about_blank() -> String {
    "about:blank".to_string()
}

async fn open_tab(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<OpenTabBody>,
) -> Result<StatusCode, ApiError> {
    state
        .config
        .policy
        .check(&body.url)
        .map_err(|err| ApiError::forbidden(err.to_string()))?;
    let agent = state.supervisor.ensure(&name).await?;
    if let Err(err) = browser_session(&agent)?.open_tab(&body.url).await {
        if err.downcast_ref::<NavigationBlocked>().is_some() {
            return Err(ApiError::forbidden(err.to_string()));
        }
        return Err(err.into());
    }
    agent.view.rebind().await;
    if let Err(err) = state.feedback.record(&name, "open_tab", &body.url).await {
        tracing::warn!("audit write failed: {err}");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn activate_tab(
    State(state): State<AppState>,
    Path((name, index)): Path<(String, usize)>,
) -> Result<StatusCode, ApiError> {
    let agent = state.supervisor.ensure(&name).await?;
    if !browser_session(&agent)?.activate_tab(index).await? {
        return Err(ApiError::not_found(format!("tab {index} not found")));
    }
    agent.view.rebind().await;
    Ok(StatusCode::NO_CONTENT)
}

async fn close_tab(
    State(state): State<AppState>,
    Path((name, index)): Path<(String, usize)>,
) -> Result<StatusCode, ApiError> {
    let agent = state.supervisor.ensure(&name).await?;
    match browser_session(&agent)?.close_tab(index).await {
        Ok(Some(changed)) => {
            if changed {
                agent.view.rebind().await;
            }
        }
        Ok(None) => {
            return Err(ApiError::not_found(format!("tab {index} not found")));
        }
        Err(err) if err.downcast_ref::<OnlyManagedTab>().is_some() => {
            return Err(ApiError::conflict(err.to_string()));
        }
        Err(err) => return Err(err.into()),
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ScreenshotQuery {
    #[serde(default)]
    full: bool,
}

async fn screenshot(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ScreenshotQuery>,
) -> Result<Response, ApiError> {
    let agent = view_session(&state.supervisor, &name).await?;
    let png = if let Some(session) = agent.backend.browser() {
        session.screenshot(query.full).await?
    } else if let Some(session) = agent.backend.desktop() {
        if query.full {
            return Err(ApiError::conflict(
                "full-page screenshots are only available for browser sessions",
            ));
        }
        session.screenshot().await?
    } else {
        return Err(ApiError::conflict(
            "terminal sessions render to cells, not pixels; read GET /v1/sessions/{name}/screen",
        ));
    };
    Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
}

/// Return a ratatui session's authoritative grid as plain text.
///
/// This is the terminal equivalent of the browser's DOM: an agent can read the
/// screen without a terminal emulator or a screenshot.
async fn screen(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    let agent = state
        .supervisor
        .existing(&name)
        .await
        .ok_or_else(|| ApiError::not_found(format!("session '{name}' not found")))?;
    let text = terminal_session(&agent)?.screen_text();
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response())
}

/// Resolve a running desktop session by name.
async fn desktop_session(
    state: &AppState,
    name: &str,
) -> Result<Arc<crate::desktop::DesktopSession>, ApiError> {
    if !is_valid_agent_name(name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    let agent = state
        .supervisor
        .existing(name)
        .await
        .ok_or_else(|| ApiError::not_found(format!("session '{name}' not found")))?;
    agent.backend.desktop().ok_or_else(|| {
        ApiError::conflict(
            "accessibility is only available for desktop sessions; browser sessions expose the DOM over CDP",
        )
    })
}

/// The session's accessibility bus, or a conflict if it publishes no tree.
fn accessibility_bus(session: &crate::desktop::DesktopSession) -> Result<&str, ApiError> {
    session
        .bus_address()
        .ok_or_else(|| ApiError::conflict("this session does not publish an accessibility tree"))
}

/// Return a desktop session's accessibility tree as JSON.
///
/// This is the desktop analogue of the terminal `/screen` endpoint: a
/// structured view an agent can read without a screenshot.
///
/// The body is the registry root node plus a `stats` object measuring what
/// the application published (`applications`, `nodes`, `named`,
/// `max_depth`), so a caller reading only the body can tell a populated
/// tree from an empty one.
///
/// A tree is not silently empty. When no application publishes on the
/// session's bus — before the application has registered, or after it
/// exited — the endpoint answers 409 rather than a bare registry skeleton.
/// When the application publishes objects but none carries a name, the tree
/// is still served (references and bounds remain actionable) with a
/// `x-lumen-tree-warning` header, and one warning is logged per session.
async fn accessibility(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let session = desktop_session(&state, &name).await?;
    let tree = accessibility::tree(accessibility_bus(&session)?).await?;
    if tree.stats.applications == 0 {
        return Err(ApiError::conflict(
            "no application is publishing an accessibility tree on this session's bus; the application may still be starting or may have exited",
        ));
    }
    let mut body = serde_json::to_value(&tree.root).context("encoding the accessibility tree")?;
    if let Some(object) = body.as_object_mut() {
        object.insert(
            "stats".into(),
            serde_json::to_value(tree.stats).context("encoding the tree stats")?,
        );
    }
    let mut response = Json(body).into_response();
    if tree.stats.named == 0 {
        if session.first_sparse_tree() {
            tracing::warn!(
                agent = name,
                objects = tree.stats.nodes,
                "the application publishes an accessibility tree without names; targets cannot be addressed by identity"
            );
        }
        let warning = format!(
            "no accessible object inside the application's windows publishes a name ({} objects)",
            tree.stats.nodes
        );
        if let Ok(value) = HeaderValue::from_str(&warning) {
            response.headers_mut().insert(TREE_WARNING_HEADER, value);
        }
    }
    Ok(response)
}

#[derive(Deserialize)]
struct AccessibilityRefBody {
    #[serde(rename = "ref")]
    reference: String,
}

/// Click the center of an element named in the accessibility tree.
///
/// The element's exact screen rectangle comes from the application itself, so
/// this is a coordinate click that survives a differently scaled screenshot.
async fn accessibility_click(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<AccessibilityRefBody>,
) -> Result<StatusCode, ApiError> {
    let session = desktop_session(&state, &name).await?;
    let tree = accessibility::tree(accessibility_bus(&session)?).await?;
    let node = tree.find(&body.reference).ok_or_else(|| {
        ApiError::not_found(format!(
            "element '{}' is not in the current accessibility tree",
            body.reference
        ))
    })?;
    let bounds = node.bounds.ok_or_else(|| {
        ApiError::conflict(format!(
            "element '{}' has no on-screen bounds",
            body.reference
        ))
    })?;
    let (x, y) = bounds.center();
    // Move first so a press never begins from a stale pointer position.
    session.mouse(MouseAction::Move, x, y, "left").await?;
    session.mouse(MouseAction::Down, x, y, "left").await?;
    session.mouse(MouseAction::Up, x, y, "left").await?;
    if let Err(err) = state
        .feedback
        .record(&name, "accessibility_click", &body.reference)
        .await
    {
        tracing::warn!("audit write failed: {err}");
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct AccessibilityTypeBody {
    text: String,
}

/// Type text into the session's focused element.
async fn accessibility_type(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<AccessibilityTypeBody>,
) -> Result<StatusCode, ApiError> {
    if body.text.is_empty() {
        return Err(ApiError::bad_request("text must not be empty"));
    }
    let session = desktop_session(&state, &name).await?;
    session.text(&body.text).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct VisibilityBody {
    visible: bool,
}

async fn set_visibility(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<VisibilityBody>,
) -> Result<StatusCode, ApiError> {
    let agent = view_session(&state.supervisor, &name).await?;
    agent.view.set_visible(body.visible).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PageScaleBody {
    scale: f64,
}

async fn set_page_scale(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<PageScaleBody>,
) -> Result<StatusCode, ApiError> {
    if !(0.0..=10.0).contains(&body.scale) || body.scale == 0.0 {
        return Err(ApiError::bad_request("scale must be within (0, 10]"));
    }
    let agent = state.supervisor.ensure(&name).await?;
    browser_session(&agent)?.set_page_scale(body.scale).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct FeedbackQuery {
    #[serde(default)]
    pending: bool,
}

async fn list_feedback(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<FeedbackQuery>,
) -> Result<Json<Vec<Feedback>>, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    Ok(Json(state.feedback.list(&name, query.pending).await?))
}

#[derive(Deserialize)]
struct FeedbackBody {
    comment: String,
    /// Base64 PNG of the region the human annotated, captured viewer-side.
    #[serde(default)]
    screenshot: Option<String>,
}

/// Decode the viewer's screenshot and reject anything that is not a bounded
/// PNG, so the feedback database only ever holds images the viewer produced.
fn decode_screenshot(encoded: &str) -> Result<Vec<u8>, ApiError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| ApiError::bad_request("screenshot must be base64-encoded"))?;
    if bytes.len() > MAX_SCREENSHOT_BYTES {
        return Err(ApiError::bad_request(format!(
            "screenshot exceeds {} bytes",
            MAX_SCREENSHOT_BYTES
        )));
    }
    if !bytes.starts_with(PNG_MAGIC) {
        return Err(ApiError::bad_request("screenshot must be a PNG"));
    }
    Ok(bytes)
}

async fn add_feedback(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<FeedbackBody>,
) -> Result<Json<Feedback>, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    if body.comment.trim().is_empty() {
        return Err(ApiError::bad_request("comment must not be empty"));
    }
    let screenshot = match body.screenshot.as_deref() {
        Some(encoded) => Some(decode_screenshot(encoded)?),
        None => None,
    };
    let feedback = state
        .feedback
        .add(&name, "human", body.comment.trim(), screenshot)
        .await?;
    if let Err(err) = state
        .feedback
        .record(&name, "feedback", body.comment.trim())
        .await
    {
        tracing::warn!("audit write failed: {err}");
    }
    Ok(Json(feedback))
}

/// Serve the PNG a note was annotated with. Missing notes and notes without an
/// image are both 404: there is nothing to render in either case.
async fn feedback_screenshot(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
) -> Result<Response, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    match state.feedback.screenshot(&name, id).await? {
        Some(png) => Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response()),
        None => Err(ApiError::not_found(format!(
            "feedback {id} has no screenshot"
        ))),
    }
}

async fn ack_feedback(
    State(state): State<AppState>,
    Path((name, id)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    if !state.feedback.ack(&name, id).await? {
        return Err(ApiError::not_found(format!("feedback {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Return a session's pending notes and acknowledge them atomically.
async fn consume_feedback(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<Feedback>>, ApiError> {
    if !is_valid_agent_name(&name) {
        return Err(ApiError::bad_request("invalid agent name"));
    }
    Ok(Json(state.feedback.consume(&name).await?))
}

async fn ack_all_feedback(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.feedback.ack_all(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CdpBody {
    method: String,
    #[serde(default)]
    params: Value,
}

async fn raw_cdp(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<CdpBody>,
) -> Result<Json<Value>, ApiError> {
    let agent = state.supervisor.ensure(&name).await?;
    let session = browser_session(&agent)?;
    let result = session
        .raw_cdp(&agent.cdp_endpoint, &body.method, body.params)
        .await?;
    if let Err(err) = state.feedback.record(&name, "cdp", &body.method).await {
        tracing::warn!("audit write failed: {err}");
    }
    Ok(Json(result))
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: i64,
}

fn default_audit_limit() -> i64 {
    100
}

async fn list_audit(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    Ok(Json(state.feedback.recent(query.limit).await?))
}

async fn stream(
    State(state): State<AppState>,
    Path(name): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| stream_session(socket, state, name))
}

async fn stream_session(socket: WebSocket, state: AppState, name: String) {
    let Some(agent) = state.supervisor.existing(&name).await else {
        tracing::debug!("stream for absent session '{name}'");
        return;
    };
    if state.is_draining() {
        return;
    }
    let mut shutdown = state.shutdown.subscribe();
    let hub: Arc<ViewHub> = agent.view.clone();
    let backend = agent.backend.clone();
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(8);
    let mut subscription = hub.subscribe().await;

    let mut writer = tokio::spawn(async move {
        loop {
            let message = tokio::select! {
                biased;
                message = out_rx.recv() => message,
                frame = subscription.next_frame() => frame.map(Message::Binary),
            };
            let Some(message) = message else {
                break;
            };
            if ws_tx.send(message).await.is_err() {
                break;
            }
        }
    });

    let _ = out_tx
        .send(Message::Text(control_event(hub.control().await).into()))
        .await;

    loop {
        let message = tokio::select! {
            message = ws_rx.next() => message,
            _ = shutdown.recv() => break,
            _ = &mut writer => break,
        };
        let Some(Ok(message)) = message else {
            break;
        };
        match message {
            Message::Text(text) => {
                if let Some(event) = handle_command(&hub, &backend, &text).await {
                    if out_tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Command {
    Mouse {
        action: String,
        x: f64,
        y: f64,
        #[serde(default)]
        button: Option<String>,
    },
    Wheel {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    Text {
        text: String,
    },
    /// A key press for terminal sessions, in the protocol's key encoding.
    Key {
        code: KeyCode,
        #[serde(default)]
        mods: u8,
    },
    Control {
        action: String,
    },
}

async fn handle_command(hub: &ViewHub, backend: &SessionBackend, raw: &str) -> Option<Message> {
    let command: Command = match serde_json::from_str(raw) {
        Ok(command) => command,
        Err(err) => return Some(error_event(format!("bad command: {err}"))),
    };

    match command {
        Command::Control { action } => {
            let owner = match action.as_str() {
                "claim" => Control::Human,
                "release" => Control::Agent,
                _ => return Some(error_event("unknown control action".into())),
            };
            hub.set_control(owner).await;
            Some(Message::Text(control_event(owner).into()))
        }
        Command::Mouse {
            action,
            x,
            y,
            button,
        } => {
            if hub.control().await != Control::Human {
                return None;
            }
            let action = match action.as_str() {
                "down" => MouseAction::Down,
                "up" => MouseAction::Up,
                "move" => MouseAction::Move,
                other => return Some(error_event(format!("unknown mouse action '{other}'"))),
            };
            let result = match backend {
                SessionBackend::Browser(session) => {
                    let kind = match action {
                        MouseAction::Down => DispatchMouseEventType::MousePressed,
                        MouseAction::Up => DispatchMouseEventType::MouseReleased,
                        MouseAction::Move => DispatchMouseEventType::MouseMoved,
                    };
                    let button = match button.as_deref() {
                        Some("right") => MouseButton::Right,
                        Some("middle") => MouseButton::Middle,
                        _ => MouseButton::Left,
                    };
                    session.mouse(kind, x, y, button).await
                }
                SessionBackend::Quickshell(session) | SessionBackend::Qt(session) => {
                    session
                        .mouse(action, x, y, button.as_deref().unwrap_or("left"))
                        .await
                }
                SessionBackend::Ratatui(session) => {
                    let kind = match action {
                        MouseAction::Down => MouseKind::Down,
                        MouseAction::Up => MouseKind::Up,
                        MouseAction::Move => MouseKind::Moved,
                    };
                    let button = match button.as_deref() {
                        Some("right") => RatatuiMouseButton::Right,
                        Some("middle") => RatatuiMouseButton::Middle,
                        Some("left") | None => RatatuiMouseButton::Left,
                        Some(_) => RatatuiMouseButton::None,
                    };
                    session.mouse(kind, x.max(0.0) as u16, y.max(0.0) as u16, button, 0)
                }
                SessionBackend::Terminal(session) => {
                    let kind = match action {
                        MouseAction::Down => MouseKind::Down,
                        MouseAction::Up => MouseKind::Up,
                        MouseAction::Move => MouseKind::Moved,
                    };
                    let button = terminal_button(button.as_deref());
                    session.mouse(kind, x.max(0.0) as u16, y.max(0.0) as u16, button, 0)
                }
            };
            match result {
                Ok(()) => None,
                Err(err) => Some(error_event(err.to_string())),
            }
        }
        Command::Wheel { x, y, dx, dy } => {
            if hub.control().await != Control::Human {
                return None;
            }
            let result = match backend {
                SessionBackend::Browser(session) => session.wheel(x, y, dx, dy).await,
                SessionBackend::Quickshell(session) | SessionBackend::Qt(session) => {
                    session.wheel(dx, dy).await
                }
                SessionBackend::Ratatui(session) => {
                    let kind = if dy >= 0.0 {
                        MouseKind::ScrollUp
                    } else {
                        MouseKind::ScrollDown
                    };
                    session.mouse(
                        kind,
                        x.max(0.0) as u16,
                        y.max(0.0) as u16,
                        RatatuiMouseButton::None,
                        0,
                    )
                }
                SessionBackend::Terminal(session) => {
                    let kind = if dy >= 0.0 {
                        MouseKind::ScrollUp
                    } else {
                        MouseKind::ScrollDown
                    };
                    session.mouse(
                        kind,
                        x.max(0.0) as u16,
                        y.max(0.0) as u16,
                        RatatuiMouseButton::None,
                        0,
                    )
                }
            };
            match result {
                Ok(()) => None,
                Err(err) => Some(error_event(err.to_string())),
            }
        }
        Command::Text { text } => {
            if hub.control().await != Control::Human {
                return None;
            }
            let result = match backend {
                SessionBackend::Browser(session) => session.insert_text(&text).await,
                SessionBackend::Quickshell(session) | SessionBackend::Qt(session) => {
                    session.text(&text).await
                }
                SessionBackend::Ratatui(session) => session.text(&text),
                SessionBackend::Terminal(session) => session.text(&text),
            };
            match result {
                Ok(()) => None,
                Err(err) => Some(error_event(err.to_string())),
            }
        }
        Command::Key { code, mods } => {
            if hub.control().await != Control::Human {
                return None;
            }
            match backend {
                SessionBackend::Ratatui(session) => match session.key(code, mods) {
                    Ok(()) => None,
                    Err(err) => Some(error_event(err.to_string())),
                },
                SessionBackend::Terminal(session) => match session.key(code, mods) {
                    Ok(()) => None,
                    Err(err) => Some(error_event(err.to_string())),
                },
                _ => Some(error_event(
                    "key input is only available for terminal sessions".into(),
                )),
            }
        }
    }
}

/// Map a viewer button name onto the protocol's mouse button.
fn terminal_button(button: Option<&str>) -> RatatuiMouseButton {
    match button {
        Some("right") => RatatuiMouseButton::Right,
        Some("middle") => RatatuiMouseButton::Middle,
        Some("left") | None => RatatuiMouseButton::Left,
        Some(_) => RatatuiMouseButton::None,
    }
}

fn control_event(owner: Control) -> String {
    json!({ "type": "control", "owner": owner }).to_string()
}

fn error_event(message: String) -> Message {
    Message::Text(
        json!({ "type": "error", "message": message })
            .to_string()
            .into(),
    )
}

/// Control-plane error mapped to a JSON response.
pub enum ApiError {
    BadRequest(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    Internal(anyhow::Error),
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        // A malformed session name is a caller error, not a service failure.
        if let Some(invalid) = err.downcast_ref::<InvalidAgentName>() {
            return Self::BadRequest(invalid.to_string());
        }
        if let Some(invalid) = err.downcast_ref::<InvalidQuickshellPath>() {
            return Self::BadRequest(invalid.to_string());
        }
        if let Some(invalid) = err.downcast_ref::<InvalidRatatuiPath>() {
            return Self::BadRequest(invalid.to_string());
        }
        if let Some(invalid) = err.downcast_ref::<InvalidTerminalCommand>() {
            return Self::BadRequest(invalid.to_string());
        }
        if let Some(invalid) = err.downcast_ref::<InvalidQtCommand>() {
            return Self::BadRequest(invalid.to_string());
        }
        if let Some(conflict) = err.downcast_ref::<SessionConflict>() {
            return Self::Conflict(conflict.to_string());
        }
        Self::Internal(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, message),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::Internal(err) => {
                tracing::warn!("control-plane error: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_loopback_hosts() {
        for host in [
            "127.0.0.1",
            "127.0.0.1:8899",
            "localhost",
            "localhost:8899",
            "[::1]:8899",
            "[::1]",
        ] {
            assert!(host_is_loopback(host), "{host} should be loopback");
        }
    }

    #[test]
    fn rejects_foreign_hosts() {
        for host in [
            "evil.test",
            "evil.test:8899",
            "10.0.0.5:8899",
            "",
            "0.0.0.0:8899",
        ] {
            assert!(!host_is_loopback(host), "{host} should not be loopback");
        }
    }

    #[test]
    fn accepts_only_loopback_origins() {
        assert!(origin_is_loopback("http://127.0.0.1:8899"));
        assert!(origin_is_loopback("http://localhost"));
        assert!(!origin_is_loopback("http://evil.test"));
        assert!(!origin_is_loopback("null"));
    }
}
