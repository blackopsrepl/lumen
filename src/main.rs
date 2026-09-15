use anyhow::Context;
use clap::{Parser, Subcommand};
use lumen::client;
use lumen::config::Config;
use lumen::http::{self, AppState};

#[derive(Parser)]
#[command(
    name = "lumen",
    version,
    about = "Single-binary browser service for agents and humans"
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
    /// Ensure a session's browser exists and print its CDP endpoint.
    Ensure {
        name: String,
        /// Human-readable label for the agent owning this session.
        #[arg(long)]
        owner: Option<String>,
    },
    /// List active sessions.
    Status,
    /// Stop a session's browser, or every session with --all.
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(),
        Command::Ensure { name, owner } => {
            let info = client::ensure(&cli.url, &name, owner.as_deref())?;
            println!("{}", info.cdp_endpoint);
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
    let server =
        axum::serve(listener, http::router(state)).with_graceful_shutdown(shutdown_signal(app));
    // The signal handler only refuses new work and releases viewers; teardown
    // runs after the server has drained, so no request can create a browser
    // behind `shutdown_all`'s back. Bounded so a wedged viewer cannot hold the
    // process open indefinitely.
    if tokio::time::timeout(std::time::Duration::from_secs(15), server)
        .await
        .is_err()
    {
        tracing::warn!("http plane did not drain in time; stopping browsers anyway");
    }
    tracing::info!("http plane drained; stopping agent browsers");
    supervisor.shutdown_all().await;
    Ok(())
}

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
