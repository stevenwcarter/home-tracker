use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use home_tracker::config::Config;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser)]
#[command(version, about = "Home inventory tracker")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the web server (the default when no subcommand is given).
    Serve,
    /// Import a Homebox backup (.zip or an exploded directory).
    Import {
        /// Path to the backup zip or directory.
        path: PathBuf,
    },
    /// Liveness probe for the container HEALTHCHECK: exit 0 if the server answers.
    Healthcheck {
        /// Port to probe; defaults to PORT from the environment.
        #[arg(long)]
        port: Option<u16>,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    home_tracker::init_tracing();
    let config = Config::from_env()?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(config).await,
        Command::Import { path } => import(config, path),
        Command::Healthcheck { port } => {
            home_tracker::healthcheck::run(port.unwrap_or(config.port))
        }
    }
}

fn import(config: Config, path: PathBuf) -> Result<()> {
    use anyhow::Context as _;
    use home_tracker::{db, import};

    let pool = db::build_pool(&config.database_url)?;
    let mut conn = pool.get().context("connection for import")?;
    db::run_migrations(&mut conn)?;
    let mut source = import::source::open(&path)?;
    tracing::info!(source = %source.describe(), data_dir = %config.data_dir.display(), "importing Homebox backup");
    let report = import::run::import_backup(&mut conn, source.as_mut(), &config.data_dir)?;
    println!("{report}");
    Ok(())
}

async fn serve(config: Config) -> Result<()> {
    use anyhow::Context as _;
    use home_tracker::{db, net, routes};

    let pool = db::build_pool(&config.database_url)?;
    {
        let mut conn = pool.get().context("connection for migrations")?;
        db::run_migrations(&mut conn)?;
        tracing::info!("migrations applied");
    }
    let originals_dir = config.data_dir.join("originals");
    std::fs::create_dir_all(&originals_dir)
        .with_context(|| format!("could not create {}", originals_dir.display()))?;

    let addr = net::listen_addr(&config.listen_address, config.port)?;
    let listener = tokio::net::TcpListener::from_std(net::bind(addr)?)
        .context("registering the listener with tokio")?;
    tracing::info!(%addr, dual_stack = addr.is_ipv6(), "listening");

    axum::serve(listener, routes::app(pool))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")
}

/// Waits for SIGINT (Ctrl-C) or, on unix, SIGTERM (what `docker stop` sends).
///
/// The `scratch` image runs this binary as PID 1, where the kernel drops
/// SIGTERM for a process unless it installs a handler; without this, every
/// `docker stop` has to wait out the full grace period and then SIGKILL.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(err) => {
                tracing::warn!(%err, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!(signal = "SIGINT", "shutting down"),
        _ = terminate => tracing::info!(signal = "SIGTERM", "shutting down"),
    }
}
