use crate::config::Config;
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

/// Build the HTTP control/view plane.
pub fn router(config: Config) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/", get(index))
        .with_state(config)
}

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn index() -> &'static str {
    "Lumen"
}
