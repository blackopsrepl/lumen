//! M0 spike: prove the Lumen CDP path end to end against a real Chromium.
//!
//! Launches an agent browser through the supervisor, drives it, captures
//! screencast frames as the page changes, and injects a click. Run with:
//!
//! ```sh
//! cargo run --example spike
//! ```

use anyhow::{Context, Result};
use base64::Engine as _;
use chromiumoxide::cdp::browser_protocol::page::EventScreencastFrame;
use futures::StreamExt;
use lumen::{cdp::CdpSession, config::Config, supervisor::Supervisor};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const PAGE: &str = "data:text/html,<title>Lumen spike</title>\
<h1 style='font:48px system-ui'>Lumen spike</h1>\
<button style='font:24px system-ui;padding:16px' \
onclick=\"document.title='clicked'\">Click me</button>";

/// Screencast only emits frames while the page is damaged, so each capture is
/// paired with a visible mutation to prove live updates, not a single paint.
const MUTATIONS: [&str; 2] = [
    "document.body.style.background='#123456'",
    "document.body.style.background='#654321'",
];

async fn save_frame(
    session: &CdpSession,
    out_dir: &Path,
    index: usize,
    frame: &EventScreencastFrame,
) -> Result<()> {
    let encoded: &str = frame.data.as_ref();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("decode screencast frame")?;
    let path = out_dir.join(format!("frame-{index}.jpg"));
    std::fs::write(&path, &bytes).context("write frame")?;
    println!(
        "frame {index}: {} bytes, {}x{} -> {}",
        bytes.len(),
        frame.metadata.device_width,
        frame.metadata.device_height,
        path.display()
    );
    session.ack_frame(frame.session_id).await
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("lumen=info,spike=info")
        .init();

    let config = Config {
        data_dir: std::env::var("LUMEN_DATA")
            .unwrap_or_else(|_| "/tmp/lumen-spike/agents".into())
            .into(),
        chrome_bin: std::env::var("LUMEN_CHROME").unwrap_or_else(|_| "chromium".into()),
        ..Config::default()
    };

    let out_dir = std::path::PathBuf::from(
        std::env::var("LUMEN_OUT").unwrap_or_else(|_| "/tmp/lumen-spike".into()),
    );
    std::fs::create_dir_all(&out_dir).context("create output dir")?;

    let viewport = config.default_viewport;
    let supervisor = Supervisor::new(Arc::new(config));
    println!("default viewport: {}x{}", viewport.width, viewport.height);

    let agent = supervisor.ensure("spike").await?;
    println!("agent 'spike' CDP endpoint: {}", agent.cdp_endpoint);

    agent.session.goto(PAGE).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    agent.session.start_screencast().await?;

    let mut frames = agent
        .session
        .current_page()
        .await
        .event_listener::<EventScreencastFrame>()
        .await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);

    let first = tokio::time::timeout_at(deadline, frames.next())
        .await
        .context("waiting for the first frame")?
        .context("screencast stream ended early")?;
    save_frame(&agent.session, &out_dir, 0, &first).await?;

    for (index, script) in MUTATIONS.iter().enumerate() {
        agent.session.current_page().await.evaluate(*script).await?;
        let frame = tokio::time::timeout_at(deadline, frames.next())
            .await
            .with_context(|| format!("waiting for frame {} after a mutation", index + 1))?
            .context("screencast stream ended early")?;
        save_frame(&agent.session, &out_dir, index + 1, &frame).await?;
    }

    agent.session.click(80.0, 100.0).await?;
    let title = agent
        .session
        .current_page()
        .await
        .evaluate("document.title")
        .await?
        .value()
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    println!("click dispatched; document.title = {title:?}");

    println!("spike ok: captured 3 live frames from a driven browser");
    Ok(())
}
