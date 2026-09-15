use crate::cdp::{CdpSession, NavigationBlocked, OnlyManagedTab, TabInfo};
use crate::config::{Config, Viewport};
use crate::feedback::{AuditEntry, Feedback, FeedbackStore, Region};
use crate::supervisor::{is_valid_agent_name, AgentInfo, InvalidAgentName, Origin, Supervisor};
use crate::view::{Control, ViewHub};
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chromiumoxide::cdp::browser_protocol::input::{DispatchMouseEventType, MouseButton};
use futures::{SinkExt, StreamExt};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

#[derive(RustEmbed)]
#[folder = "ui/"]
struct UiAssets;

/// Shared control-plane state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub supervisor: Arc<Supervisor>,
    pub feedback: Arc<FeedbackStore>,
}

impl AppState {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let feedback = FeedbackStore::open(&config.feedback_db, config.audit_retain)?;
        let config = Arc::new(config);
        let supervisor = Arc::new(Supervisor::new(config.clone())?);
        supervisor.spawn_janitor();
        Ok(Self {
            supervisor,
            feedback: Arc::new(feedback),
            config,
        })
    }
}

/// Build the HTTP control/view plane.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/style.css", get(style_css))
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
        .route("/v1/sessions/{name}/feedback/{id}/ack", post(ack_feedback))
        .route(
            "/v1/sessions/{name}/feedback/ack-all",
            post(ack_all_feedback),
        )
        .with_state(state)
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

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<AgentInfo>> {
    Json(state.supervisor.list().await)
}

#[derive(Deserialize)]
struct CreateSession {
    name: String,
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
    let origin = body.origin.unwrap_or(Origin::Manual);
    let owner = body.owner.filter(|value| !value.trim().is_empty());
    let agent = state
        .supervisor
        .ensure_as(&body.name, Some((origin, owner)))
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
    if let Err(err) = agent.session.goto(&body.url).await {
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
    agent.session.set_viewport(body.width, body.height).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_viewport(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let agent = state.supervisor.ensure(&name).await?;
    let Viewport { width, height } = state.config.default_viewport;
    agent.session.set_viewport(width, height).await?;
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
    Ok(Json(agent.session.tabs().await?))
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
    if let Err(err) = agent.session.open_tab(&body.url).await {
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
    if !agent.session.activate_tab(index).await? {
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
    match agent.session.close_tab(index).await {
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
    let agent = state.supervisor.ensure(&name).await?;
    let png = agent.session.screenshot(query.full).await?;
    Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
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
    let agent = state.supervisor.ensure(&name).await?;
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
    agent.session.set_page_scale(body.scale).await?;
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
    #[serde(default)]
    region: Option<Region>,
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
    let feedback = state
        .feedback
        .add(&name, "human", body.comment.trim(), body.region)
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
    let result = agent
        .session
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
    let hub: Arc<ViewHub> = agent.view.clone();
    let session: Arc<CdpSession> = agent.session.clone();
    let mut frames = hub.subscribe().await;

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(64);

    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if ws_tx.send(message).await.is_err() {
                break;
            }
        }
    });

    let frame_tx = out_tx.clone();
    let frame_task = tokio::spawn(async move {
        loop {
            match frames.recv().await {
                Ok(bytes) => {
                    if frame_tx
                        .send(Message::Binary(Bytes::from(bytes.to_vec())))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // A page only screencasts on change; hand the joiner the last known frame
    // so an idle page does not leave the viewer staring at an empty canvas.
    if let Some(latest) = hub.latest_frame().await {
        let _ = out_tx
            .send(Message::Binary(Bytes::from(latest.to_vec())))
            .await;
    }

    let _ = out_tx
        .send(Message::Text(control_event(hub.control().await).into()))
        .await;

    while let Some(Ok(message)) = ws_rx.next().await {
        match message {
            Message::Text(text) => {
                if let Some(event) = handle_command(&hub, &session, &text).await {
                    if out_tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    frame_task.abort();
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
    Control {
        action: String,
    },
}

async fn handle_command(hub: &ViewHub, session: &CdpSession, raw: &str) -> Option<Message> {
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
            let kind = match action.as_str() {
                "down" => DispatchMouseEventType::MousePressed,
                "up" => DispatchMouseEventType::MouseReleased,
                "move" => DispatchMouseEventType::MouseMoved,
                other => return Some(error_event(format!("unknown mouse action '{other}'"))),
            };
            let button = match button.as_deref() {
                Some("right") => MouseButton::Right,
                Some("middle") => MouseButton::Middle,
                _ => MouseButton::Left,
            };
            match session.mouse(kind, x, y, button).await {
                Ok(()) => None,
                Err(err) => Some(error_event(err.to_string())),
            }
        }
        Command::Wheel { x, y, dx, dy } => {
            if hub.control().await != Control::Human {
                return None;
            }
            match session.wheel(x, y, dx, dy).await {
                Ok(()) => None,
                Err(err) => Some(error_event(err.to_string())),
            }
        }
        Command::Text { text } => {
            if hub.control().await != Control::Human {
                return None;
            }
            match session.insert_text(&text).await {
                Ok(()) => None,
                Err(err) => Some(error_event(err.to_string())),
            }
        }
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
