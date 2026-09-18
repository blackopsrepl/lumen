use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub kind: String,
    pub cdp_endpoint: String,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Feedback {
    id: i64,
    author: String,
    comment: String,
    /// Whether the note carries a region screenshot the agent can fetch.
    screenshot: bool,
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

/// Ensure a browser session exists and return its CDP endpoint.
///
/// This registers the session as agent-owned, so the viewer can tell it apart
/// from a browser a human created with nobody attached.
pub fn ensure(base: &str, name: &str, owner: Option<&str>) -> Result<AgentInfo> {
    ensure_session(base, name, "browser", None, owner)
}

pub fn ensure_quickshell(
    base: &str,
    name: &str,
    path: &str,
    owner: Option<&str>,
) -> Result<AgentInfo> {
    ensure_session(base, name, "quickshell", Some(path), owner)
}

pub fn ensure_ratatui(
    base: &str,
    name: &str,
    path: &str,
    owner: Option<&str>,
) -> Result<AgentInfo> {
    ensure_session(base, name, "ratatui", Some(path), owner)
}

fn ensure_session(
    base: &str,
    name: &str,
    kind: &str,
    path: Option<&str>,
    owner: Option<&str>,
) -> Result<AgentInfo> {
    let response = client()?
        .post(format!("{base}/v1/sessions"))
        .json(&serde_json::json!({
            "name": name,
            "kind": kind,
            "path": path,
            "origin": "agent",
            "owner": owner
        }))
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
        if session.kind == "quickshell" {
            println!(
                "{:<24} quickshell {}",
                session.name,
                session.path.as_deref().unwrap_or("(unknown path)")
            );
        } else if session.kind == "ratatui" {
            println!(
                "{:<24} ratatui {}",
                session.name,
                session.path.as_deref().unwrap_or("(unknown path)")
            );
        } else {
            println!("{:<24} {}", session.name, session.cdp_endpoint);
        }
    }
    Ok(())
}

/// Stop and forget a session.
///
/// Stopping a session that is already gone is not an error: `close` may run
/// twice, or after the service reaped a crashed browser.
pub fn stop(base: &str, name: &str) -> Result<()> {
    let response = client()?
        .delete(format!("{base}/v1/sessions/{name}"))
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        println!("session {name} not running");
        return Ok(());
    }
    check(response)?;
    println!("stopped {name}");
    Ok(())
}

/// Stop every active session.
pub fn stop_all(base: &str) -> Result<()> {
    let sessions: Vec<AgentInfo> = check(client()?.get(format!("{base}/v1/sessions")).send()?)
        .context("connecting to lumen")?
        .json()
        .context("parsing sessions")?;
    if sessions.is_empty() {
        println!("no active sessions");
        return Ok(());
    }
    for session in &sessions {
        stop(base, &session.name)?;
    }
    Ok(())
}

/// Print a session's pending human feedback, optionally consuming it.
///
/// `--consume` uses the atomic endpoint: the notes it prints are exactly the
/// notes it acknowledges, so one arriving mid-read is never silently lost.
pub fn feedback(base: &str, name: &str, consume: bool) -> Result<()> {
    let items: Vec<Feedback> = if consume {
        check(
            client()?
                .post(format!("{base}/v1/sessions/{name}/feedback/consume"))
                .send()?,
        )
        .context("connecting to lumen")?
        .json()
        .context("parsing feedback")?
    } else {
        check(
            client()?
                .get(format!("{base}/v1/sessions/{name}/feedback?pending=true"))
                .send()?,
        )
        .context("connecting to lumen")?
        .json()
        .context("parsing feedback")?
    };

    if items.is_empty() {
        println!("no pending feedback for {name}");
        return Ok(());
    }

    for item in &items {
        println!("[{}] {}: {}", item.id, item.author, item.comment);
        if item.screenshot {
            report_screenshot(base, name, item.id);
        }
    }

    if consume {
        println!("consumed {} note(s)", items.len());
    }
    Ok(())
}

/// Fetch a note's screenshot and save it where the agent can read it.
///
/// The image lives in the service, not the caller's filesystem, so a failure to
/// save is not fatal: the endpoint that serves it is always printed as a
/// fallback. `LUMEN_FEEDBACK_DIR` chooses the destination directory.
fn report_screenshot(base: &str, name: &str, id: i64) {
    let endpoint = format!("{base}/v1/sessions/{name}/feedback/{id}/screenshot");
    let dir = std::env::var_os("LUMEN_FEEDBACK_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("lumen-feedback"));
    let saved = std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))
        .and_then(|()| {
            let bytes = check(client()?.get(&endpoint).send()?)
                .context("connecting to lumen")?
                .bytes()
                .context("reading screenshot")?;
            let path = dir.join(format!("{name}-{id}.png"));
            std::fs::write(&path, &bytes).with_context(|| format!("writing {}", path.display()))?;
            Ok(std::fs::canonicalize(&path).unwrap_or(path))
        });
    match saved {
        Ok(path) => println!("     screenshot: {} ({endpoint})", path.display()),
        Err(err) => println!("     screenshot not saved ({err}); fetch {endpoint}"),
    }
}
