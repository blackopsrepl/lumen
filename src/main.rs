use anyhow::Context;
use lumen::config::Config;
use lumen::http::{self, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumen=info".into()),
        )
        .init();

    let addr = config.bind_addr();
    tracing::info!(
        "lumen listening on http://{addr} (default viewport {}x{})",
        config.default_viewport.width,
        config.default_viewport.height
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    axum::serve(listener, http::router(AppState::new(config)?))
        .await
        .context("http server failed")?;
    Ok(())
}
