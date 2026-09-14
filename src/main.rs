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
    /// Stop a session's browser.
    Stop { name: String },
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
        Command::Stop { name } => client::stop(&cli.url, &name),
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
