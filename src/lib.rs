#![warn(clippy::str_to_string)]

pub mod api;
pub mod asset_id;
pub mod config;
pub mod db;
pub mod graphql;
pub mod healthcheck;
pub mod kinds;
pub mod models;
pub mod money;
pub mod net;
pub mod routes;
pub mod schema;
pub mod svc;

use tracing_subscriber::{EnvFilter, fmt};

/// Installs the global tracing subscriber, filtered by `RUST_LOG` (default `info`).
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();
}
