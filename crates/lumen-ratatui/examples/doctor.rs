//! A ratatui dashboard that diagnoses a running Lumen by querying its own API.
//!
//! Run it under Lumen itself, so the thing being observed and the thing doing
//! the observing are the same service:
//!
//! ```text
//! cargo build --release -p lumen-ratatui --example doctor
//! lumen ensure doctor --ratatui "$PWD/target/release/examples/doctor" \
//!   --owner lumen-self-check
//! ```
//!
//! The service URL comes from `LUMEN_URL` (default `http://127.0.0.1:8899`).
//! The app polls `/healthz`, `/v1/sessions`, and `/v1/audit`, so the screen
//! shows the service's own view of the world. `r` refreshes, `q`/`Esc` quit.

use lumen_ratatui::{Event, Session};
use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(500);
const REFRESH: Duration = Duration::from_secs(2);

enum Probe {
    Ok(String),
    Err(String),
}

struct Doctor {
    url: String,
    health: Probe,
    sessions: Probe,
    audit: Probe,
    checked_at: Option<Instant>,
    last_error: Option<String>,
    ticks: u64,
}

impl Doctor {
    fn new(url: String) -> Self {
        Self {
            url,
            health: Probe::Err("not checked yet".into()),
            sessions: Probe::Err("not checked yet".into()),
            audit: Probe::Err("not checked yet".into()),
            checked_at: None,
            last_error: None,
            ticks: 0,
        }
    }

    /// One HTTP GET, returning the response body. Only http and a loopback host
    /// are supported; a diagnostic should not become a general web client.
    fn get(path: &str) -> Result<String, String> {
        let url = format!(
            "{}{}",
            std::env::var("LUMEN_URL").unwrap_or_else(|_| "http://127.0.0.1:8899".into()),
            path
        );
        let rest = url
            .strip_prefix("http://")
            .ok_or("only http:// is supported")?;
        let (authority, path) = match rest.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (rest, "/".to_string()),
        };
        let mut stream =
            TcpStream::connect(authority).map_err(|err| format!("connect {authority}: {err}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|err| err.to_string())?;
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
        )
        .map_err(|err| err.to_string())?;
        let mut reader = BufReader::new(stream);
        let mut status = String::new();
        reader
            .read_line(&mut status)
            .map_err(|err| err.to_string())?;
        let code = status
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| format!("bad status line: {status:?}"))?
            .to_string();
        let mut content_length = None;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).map_err(|err| err.to_string())?;
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
        let mut body = String::new();
        reader
            .read_to_string(&mut body)
            .map_err(|err| err.to_string())?;
        if code != "200" {
            return Err(format!("HTTP {code}"));
        }
        if let Some(len) = content_length {
            body.truncate(len.min(body.len()));
        }
        Ok(body)
    }

    fn refresh(&mut self) {
        self.health = match Self::get("/healthz") {
            Ok(body) => Probe::Ok(one_line(&body)),
            Err(err) => Probe::Err(err),
        };
        self.sessions = match Self::get("/v1/sessions") {
            Ok(body) => Probe::Ok(summarize_sessions(&body)),
            Err(err) => Probe::Err(err),
        };
        self.audit = match Self::get("/v1/audit?limit=5") {
            Ok(body) => Probe::Ok(summarize_audit(&body)),
            Err(err) => Probe::Err(err),
        };
        self.checked_at = Some(Instant::now());
        self.last_error = None;
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                " Lumen doctor ".bold().fg(Color::Black).bg(Color::Cyan),
                "  the service watching itself ".fg(Color::DarkGray),
            ])),
            rows[0],
        );

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    "endpoint  ".fg(Color::DarkGray),
                    self.url.clone().fg(Color::White),
                ]),
                Line::from(vec![
                    "healthz   ".fg(Color::DarkGray),
                    probe_line(&self.health),
                ]),
            ])
            .block(Block::new().borders(Borders::ALL).title(" service ")),
            rows[1],
        );

        let mid = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(rows[2]);
        frame.render_widget(
            Paragraph::new(vec![Line::from(probe_line(&self.sessions))])
                .wrap(ratatui::widgets::Wrap { trim: false })
                .block(
                    Block::new()
                        .borders(Borders::ALL)
                        .title(" GET /v1/sessions "),
                ),
            mid[0],
        );
        frame.render_widget(
            Paragraph::new(vec![Line::from(probe_line(&self.audit))])
                .wrap(ratatui::widgets::Wrap { trim: false })
                .block(
                    Block::new()
                        .borders(Borders::ALL)
                        .title(" GET /v1/audit?limit=5 "),
                ),
            mid[1],
        );

        let age = self
            .checked_at
            .map(|at| format!("{:.1}s ago", at.elapsed().as_secs_f32()))
            .unwrap_or_else(|| "never".into());
        let mut status = Line::from(vec![
            " r ".bold().fg(Color::Black).bg(Color::Yellow),
            format!(" refresh   age {age}   ").fg(Color::DarkGray),
            format!("ticks {}   ", self.ticks).fg(Color::DarkGray),
        ]);
        if let Some(err) = &self.last_error {
            status.push_span(err.clone().fg(Color::LightRed));
        }
        frame.render_widget(Paragraph::new(status), rows[3]);
    }
}

fn probe_line(probe: &Probe) -> ratatui::text::Span<'static> {
    match probe {
        Probe::Ok(text) => {
            ratatui::text::Span::styled(text.clone(), Style::default().fg(Color::Green))
        }
        Probe::Err(err) => ratatui::text::Span::styled(
            format!("ERROR: {err}"),
            Style::default().fg(Color::LightRed),
        ),
    }
}

fn one_line(body: &str) -> String {
    body.trim().replace('\n', " ")
}

/// Reduce the sessions JSON to one readable line without a JSON dependency.
fn summarize_sessions(body: &str) -> String {
    let names: Vec<&str> = body
        .match_indices("\"name\":")
        .map(|(i, _)| i)
        .map(|i| {
            let rest = &body[i + 7..];
            let rest = rest.trim_start().trim_start_matches('"');
            rest.split('"').next().unwrap_or("?")
        })
        .collect();
    let kinds: Vec<&str> = body
        .match_indices("\"kind\":")
        .map(|(i, _)| i)
        .map(|i| {
            let rest = &body[i + 7..];
            let rest = rest.trim_start().trim_start_matches('"');
            rest.split('"').next().unwrap_or("?")
        })
        .collect();
    if names.is_empty() {
        return format!("{} (no sessions)", one_line(body));
    }
    let list: Vec<String> = names
        .iter()
        .zip(kinds.iter())
        .map(|(name, kind)| format!("{name}[{kind}]"))
        .collect();
    format!("{} session(s): {}", names.len(), list.join(", "))
}

/// Reduce the audit JSON to a count and the newest actions.
fn summarize_audit(body: &str) -> String {
    let actions: Vec<&str> = body
        .match_indices("\"action\":")
        .map(|(i, _)| i)
        .map(|i| {
            let rest = &body[i + 9..];
            let rest = rest.trim_start().trim_start_matches('"');
            rest.split('"').next().unwrap_or("?")
        })
        .collect();
    if actions.is_empty() {
        return format!("{} (empty)", one_line(body));
    }
    format!("{} entr(ies): {}", actions.len(), actions.join(", "))
}

fn main() -> std::io::Result<()> {
    if std::env::var_os("LUMEN_COLS").is_none() {
        eprintln!("doctor: no Lumen session detected; run it through the service");
        std::process::exit(1);
    }
    let url = std::env::var("LUMEN_URL").unwrap_or_else(|_| "http://127.0.0.1:8899".into());
    let mut session = Session::connect()?;
    let mut doctor = Doctor::new(url);
    doctor.refresh();
    let mut last = Instant::now();

    loop {
        session.draw(|frame| doctor.render(frame))?;

        if let Some(event) = session.poll(POLL) {
            match event {
                Event::Quit => break,
                Event::Key(key) => {
                    use lumen_ratatui::protocol::KeyCode;
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('r') => doctor.refresh(),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        doctor.ticks += 1;
        if last.elapsed() >= REFRESH {
            doctor.refresh();
            last = Instant::now();
        }
    }
    Ok(())
}
