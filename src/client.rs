use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub cdp_endpoint: String,
}

#[derive(Debug, Deserialize)]
struct Region {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
}

#[derive(Debug, Deserialize)]
struct Feedback {
    id: i64,
    author: String,
    comment: String,
    region: Option<Region>,
}

fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .context("building http client")
}

fn check(response: reqwest::blocking::Response) -> Result<reqwest::blocking::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().unwrap_or_default();
    bail!("lumen returned {status}: {body}");
}

/// Ensure a session's browser exists and return its CDP endpoint.
pub fn ensure(base: &str, name: &str) -> Result<AgentInfo> {
    let response = client()?
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({ "name": name }))
        .send()
        .context("connecting to lumen")?;
    check(response)?.json().context("parsing session")
}

pub fn status(base: &str) -> Result<()> {
    let sessions: Vec<AgentInfo> = check(client()?.get(format!("{base}/v1/sessions")).send()?)
        .context("connecting to lumen")?
        .json()
        .context("parsing sessions")?;
    if sessions.is_empty() {
        println!("no active sessions");
    }
    for session in sessions {
        println!("{:<24} {}", session.name, session.cdp_endpoint);
    }
    Ok(())
}

/// Stop and forget a session's browser.
pub fn stop(base: &str, name: &str) -> Result<()> {
    check(
        client()?
            .delete(format!("{base}/v1/sessions/{name}"))
            .send()?,
    )?;
    println!("stopped {name}");
    Ok(())
}

/// Print a session's pending human feedback, optionally consuming it.
pub fn feedback(base: &str, name: &str, consume: bool) -> Result<()> {
    let items: Vec<Feedback> = check(
        client()?
            .get(format!("{base}/v1/sessions/{name}/feedback?pending=true"))
            .send()?,
    )
    .context("connecting to lumen")?
    .json()
    .context("parsing feedback")?;

    if items.is_empty() {
        println!("no pending feedback for {name}");
        return Ok(());
    }

    for item in &items {
        println!("[{}] {}: {}", item.id, item.author, item.comment);
        if let Some(region) = &item.region {
            println!(
                "     region x={:.0} y={:.0} w={:.0} h={:.0} (view scale {:.2})",
                region.x, region.y, region.width, region.height, region.scale
            );
        }
    }

    if consume {
        check(
            client()?
                .post(format!("{base}/v1/sessions/{name}/feedback/ack-all"))
                .send()?,
        )?;
        println!("consumed {} note(s)", items.len());
    }
    Ok(())
}
