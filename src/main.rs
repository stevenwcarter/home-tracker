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
        Command::Serve => {
            tracing::info!(?config, "serve: not implemented yet");
            Ok(())
        }
        Command::Import { path } => {
            tracing::info!(?path, "import: not implemented yet");
            Ok(())
        }
        Command::Healthcheck { port } => {
            tracing::info!(
                port = port.unwrap_or(config.port),
                "healthcheck: not implemented yet"
            );
            Ok(())
        }
    }
}
