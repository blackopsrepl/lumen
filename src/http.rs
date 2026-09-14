use crate::config::{Config, Viewport};
use crate::supervisor::{is_valid_agent_name, AgentInfo, Supervisor};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Shared control-plane state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub supervisor: Arc<Supervisor>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        Self {
            supervisor: Arc::new(Supervisor::new(config.clone())),
            config,
        }
    }
}

/// Build the HTTP control/view plane.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/sessions", get(list_sessions).post(create_session))
        .route("/v1/sessions/{name}", get(get_session))
        .route(
            "/v1/sessions/{name}/viewport",
            put(set_viewport).delete(reset_viewport),
        )
        .with_state(state)
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
}

async fn create_session(
    State(state): State<AppState>,
    Json(body): Json<CreateSession>,
) -> Result<Json<AgentInfo>, ApiError> {
    let info = ensure(&state, &body.name).await?;
    Ok(Json(info))
}

async fn get_session(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<AgentInfo>, ApiError> {
    Ok(Json(ensure(&state, &name).await?))
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

async fn ensure(state: &AppState, name: &str) -> Result<AgentInfo, ApiError> {
    if !is_valid_agent_name(name) {
        return Err(ApiError::bad_request(
            "invalid agent name (use [A-Za-z0-9._-], 1-32 chars)",
        ));
    }
    let agent = state.supervisor.ensure(name).await?;
    Ok(AgentInfo {
        name: agent.name.clone(),
        cdp_endpoint: agent.cdp_endpoint.clone(),
    })
}

/// Control-plane error mapped to a JSON response.
pub enum ApiError {
    BadRequest(String),
    Internal(anyhow::Error),
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Internal(err) => {
                tracing::warn!("control-plane error: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
