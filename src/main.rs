mod config;
mod http;

use anyhow::Context;
use config::Config;

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
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!(
        "lumen listening on http://{addr} (default viewport {}x{})",
        config.default_viewport.width,
        config.default_viewport.height
    );

    axum::serve(listener, http::router(config))
        .await
        .context("http server failed")?;
    Ok(())
}
