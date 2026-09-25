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
        Command::Import { path } => {
            anyhow::bail!(
                "import is not implemented yet (planned for phase 2); got {}",
                path.display()
            )
        }
        Command::Healthcheck { port } => {
            home_tracker::healthcheck::run(port.unwrap_or(config.port))
        }
    }
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
    std::fs::create_dir_all(config.data_dir.join("originals"))
        .with_context(|| format!("could not create {}", config.data_dir.display()))?;

    let addr = net::listen_addr(&config.listen_address, config.port)?;
    let listener = tokio::net::TcpListener::from_std(net::bind(addr)?)
        .context("registering the listener with tokio")?;
    tracing::info!(%addr, dual_stack = addr.is_ipv6(), "listening");

    axum::serve(listener, routes::app(pool))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await
        .context("server error")
}
