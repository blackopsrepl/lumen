use anyhow::Context;
use clap::{Parser, Subcommand};
use lumen::client;
use lumen::config::Config;
use lumen::http::{self, AppState};
use std::future::IntoFuture;

#[derive(Parser)]
#[command(
    name = "lumen",
    version,
    about = "Single-binary session service for agents and humans"
)]
struct Cli {
    /// Base URL of a running Lumen.
    #[arg(
        long,
        global = true,
        env = "LUMEN_URL",
        default_value = "http://127.0.0.1:8899"
    )]
    url: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the service (default when no subcommand is given).
    Serve,
    /// Ensure a session exists and print its endpoint or desktop path.
    Ensure {
        name: String,
        /// Human-readable label for the agent owning this session.
        #[arg(long)]
        owner: Option<String>,
        /// Start a Quickshell session from this file or configuration directory.
        #[arg(long, value_name = "PATH")]
        quickshell: Option<String>,
        /// Start a Ratatui session from this app binary.
        #[arg(long, value_name = "PATH")]
        ratatui: Option<String>,
        /// Start a Qt application session from this command: an absolute
        /// program plus arguments.
        #[arg(long, value_name = "COMMAND")]
        qt: Option<String>,
    },
    /// List active sessions.
    Status,
    /// Stop a session, or every session with --all.
    Stop {
        name: Option<String>,
        /// Stop every active session.
        #[arg(long)]
        all: bool,
    },
    /// Print a session's pending human feedback.
    Feedback {
        name: String,
        /// Acknowledge the printed notes after showing them.
        #[arg(long)]
        consume: bool,
    },
    /// Print a desktop session's accessibility tree as JSON.
    Accessibility { name: String },
    /// Click an element in a desktop session by its accessibility reference.
    Click { name: String, reference: String },
    /// Type text into a desktop session's focused element.
    Type { name: String, text: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(),
        Command::Ensure {
            name,
            owner,
            quickshell,
            ratatui,
            qt,
        } => {
            let selected = [quickshell.is_some(), ratatui.is_some(), qt.is_some()]
                .into_iter()
                .filter(|present| *present)
                .count();
            if selected > 1 {
                anyhow::bail!("choose at most one of --quickshell, --ratatui, or --qt");
            }
            let info = if let Some(path) = quickshell {
                client::ensure_quickshell(&cli.url, &name, &path, owner.as_deref())?
            } else if let Some(path) = ratatui {
                client::ensure_ratatui(&cli.url, &name, &path, owner.as_deref())?
            } else if let Some(command) = qt {
                client::ensure_qt(&cli.url, &name, &command, owner.as_deref())?
            } else {
                client::ensure(&cli.url, &name, owner.as_deref())?
            };
            match info.kind.as_str() {
                "quickshell" | "ratatui" | "qt" => println!(
                    "{} {}",
                    info.kind,
                    info.path.as_deref().unwrap_or("(unknown path)")
                ),
                _ => println!("{}", info.cdp_endpoint),
            }
            Ok(())
        }
        Command::Status => client::status(&cli.url),
        Command::Stop { name, all } => match (all, name) {
            (true, _) => client::stop_all(&cli.url),
            (false, Some(name)) => client::stop(&cli.url, &name),
            (false, None) => {
                anyhow::bail!("stop needs a session name, or --all to stop every session")
            }
        },
        Command::Feedback { name, consume } => client::feedback(&cli.url, &name, consume),
        Command::Accessibility { name } => client::accessibility(&cli.url, &name),
        Command::Click { name, reference } => client::click(&cli.url, &name, &reference),
        Command::Type { name, text } => client::type_text(&cli.url, &name, &text),
    }
}

fn serve() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building the async runtime")?
        .block_on(serve_async())
}

async fn serve_async() -> anyhow::Result<()> {
    let config = Config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lumen=info".into()),
        )
        .init();

    let addr = config.bind_addr()?;
    tracing::info!(
        "lumen listening on http://{addr} (default viewport {}x{})",
        config.default_viewport.width,
        config.default_viewport.height
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    let state = AppState::new(config)?;
    let supervisor = state.supervisor.clone();
    let app = state.clone();
    let server = axum::serve(listener, http::router(state))
        .with_graceful_shutdown(shutdown_signal(app.clone()))
        .into_future();
    tokio::pin!(server);

    // The grace period begins only once a signal has asked the plane to drain;
    // it must never bound the server's ordinary lifetime. The signal handler
    // just refuses new work and releases viewers, so teardown cannot race a
    // request that creates a browser.
    let outcome = tokio::select! {
        result = &mut server => result.context("http server failed"),
        _ = async {
            app.drain_started().await;
            tokio::time::sleep(DRAIN_GRACE).await;
        } => {
            tracing::warn!(
                "http plane did not drain within {}s; stopping sessions anyway",
                DRAIN_GRACE.as_secs()
            );
            Ok(())
        }
    };

    tracing::info!("stopping agent sessions");
    supervisor.shutdown_all().await;
    outcome
}

/// How long the HTTP plane may take to drain after shutdown begins.
const DRAIN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// Resolve once SIGINT or SIGTERM arrives, after telling the server to start
/// draining.
async fn shutdown_signal(state: AppState) {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("installing the SIGTERM handler");
    tokio::select! {
        _ = ctrl_c => {},
        _ = term.recv() => {},
    }
    tracing::info!("shutdown signal received; draining");
    state.begin_draining();
}
