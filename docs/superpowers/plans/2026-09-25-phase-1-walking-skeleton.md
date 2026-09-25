# Phase 1: Walking Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A deployable Rust/Axum + React app where the home page shows four statistic cards fed by a real GraphQL `summary` query that returns fixed dummy numbers plus the currency read from SQLite.

**Architecture:** Single Rust crate at the repo root (`home-tracker`) with clap subcommands `serve` (default), `import` (stub) and `healthcheck`; Diesel/SQLite with embedded migrations; juniper schema behind `/graphql`; the Vite-built React site embedded with rust-embed. The frontend is `site/`, one hook per GraphQL operation, Tailwind v4 with semantic theme tokens, dark default.

**Tech Stack:** Rust 2024, axum 0.8, juniper 0.17 + juniper_axum 0.3, diesel 2.3 (sqlite, r2d2, bundled libsqlite3), clap 4 derive, tracing, rust-embed 8, axum-test 21, tempfile; React 19, TypeScript, Vite 8, Apollo Client 4, Tailwind v4, vitest 4, testing-library; yarn 1; husky + lint-staged; Docker musl/scratch; GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-09-25-home-tracker-design.md` (sections 4, 6, 7, 10, 11, 12, 13, 15 "Phase 1").

## Global Constraints

- Rust `edition = "2024"` in `Cargo.toml`; `rustfmt.toml` contains exactly `edition = "2024"`.
- OpenSSL-free: no `openssl`/`native-tls` anywhere in the graph. Phase 1 has no HTTP client at all. Verify with `cargo tree -i openssl` and `cargo tree -i native-tls` (both must print "nothing to print" / error "package not found").
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` must pass at every commit.
- Always commit `Cargo.lock` with any dependency change.
- Port default `7008`; `LISTEN_ADDRESS` default `::`; `DATABASE_URL` default `data/db.sqlite`; `DATA_DIR` default `data`; `RUST_LOG` default `info`.
- GraphQL: Rust `snake_case` fields, juniper converts to `camelCase`. No `is` prefix on booleans except `isLocation`.
- Frontend: `yarn` only, never `npm`. Components use only semantic colour classes (`bg-bg`, `bg-surface`, `bg-surface-raised`, `border-border`, `text-text`, `text-muted`, `bg-accent`, `text-accent`, `text-accent-text`, `text-danger`, `text-success`), never raw palette classes like `bg-zinc-900`.
- Every GraphQL operation lives in exactly one hook under `site/src/hooks/`; components never import Apollo.
- Frontend errors surface via `toast.error`.
- The `homebox-backup/` and `homebox/` directories are excluded through `.git/info/exclude`; never `git add` them.
- Commit messages end with `Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF`.

## Review Focus

Failure modes the spec implies that no task's happy path exercises. Each has a test in the task named.

1. `PORT=abc` or `PORT=70000`: the server must refuse to start with a message naming `PORT`, not silently bind 7008. Task 1 `config` tests.
2. `DATABASE_URL=/nonexistent/dir/db.sqlite`: the parent directory is created and startup succeeds. Task 2 `db` test.
3. The `currency` settings row is missing (someone deleted it): `summary` returns a GraphQL error, not a panic and not a silent `USD`. Task 3 `svc::stats` test.
4. A deep link like `/locations/abc` on a fresh page load must serve `index.html` (200, `text/html`), not 404, or client routing never boots. Task 3 routes test.
5. `localStorage` holds a theme value that is not `dark` or `light` (or throws in a private window): the app must render dark, not crash. Task 6 `useTheme` tests.

---

### Task 1: Crate manifest, config, network binding, CLI skeleton

**Files:**
- Modify: `Cargo.toml`
- Create: `rustfmt.toml`, `diesel.toml`, `.gitignore` (replace), `src/lib.rs`, `src/config.rs`, `src/net.rs`, `src/main.rs` (replace)

**Interfaces:**
- Produces: `home_tracker::config::Config { port: u16, listen_address: String, database_url: String, data_dir: PathBuf }`, `Config::from_env() -> anyhow::Result<Config>`, `Config::from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Config>`.
- Produces: `home_tracker::net::listen_addr(listen_address: &str, port: u16) -> anyhow::Result<SocketAddr>`, `home_tracker::net::bind(addr: SocketAddr) -> anyhow::Result<std::net::TcpListener>`.
- Produces: `home_tracker::init_tracing()`.
- Produces: the clap `Cli`/`Command` enum in `main.rs` with `Serve`, `Import { path: PathBuf }`, `Healthcheck { port: Option<u16> }`. Tasks 3 and 4 replace the bodies.

- [ ] **Step 1: Write the manifest and tooling files**

`Cargo.toml`:

```toml
[package]
name = "home-tracker"
version = "0.1.0"
edition = "2024"
default-run = "home-tracker"

[profile.release]
codegen-units = 1
opt-level = 3
lto = true

[dependencies]
anyhow = "1"
axum = { version = "0.8", features = ["http2", "macros"] }
chrono = { version = "0.4", features = ["serde", "clock"] }
clap = { version = "4", features = ["derive"] }
diesel = { version = "2.3", features = ["sqlite", "r2d2", "chrono", "returning_clauses_for_sqlite_3_35"] }
diesel_migrations = { version = "2", features = ["sqlite"] }
dotenvy = "0.15"
juniper = { version = "0.17", features = ["anyhow", "chrono", "uuid"] }
juniper_axum = "0.3"
libsqlite3-sys = { version = "0.35", features = ["bundled"] }
mime_guess = "2"
rust-embed = { version = "8", features = ["axum", "compression"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
socket2 = "0.6"
tempfile = "3"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal", "sync", "net", "fs"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["compression-gzip", "compression-br", "compression-zstd", "compression-deflate"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v7", "serde"] }

[target.'cfg(target_env = "musl")'.dependencies]
tikv-jemallocator = "0.6"

[dev-dependencies]
axum-test = "21"
```

If `libsqlite3-sys = "0.35"` does not resolve against the chosen diesel, run `cargo tree -p diesel -e features -i libsqlite3-sys` after the first `cargo build` and pin to the version diesel actually pulls; the `bundled` feature is the point.

`rustfmt.toml`:

```toml
edition = "2024"
```

`diesel.toml`:

```toml
[print_schema]
file = "src/schema.rs"

[migrations_directory]
dir = "migrations"
```

`.gitignore` (replace the whole file):

```
/target
/data
/db
*.sqlite
*.sqlite-*
lcov*
cobertura.xml
.env
.env.*
!env.prod
node_modules
site/build
site/coverage
site/.vite
*.log
.DS_Store
.worktrees/
```

- [ ] **Step 2: Write the failing config tests**

`src/config.rs`:

```rust
//! Process configuration read from environment variables.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

pub const DEFAULT_PORT: u16 = 7008;
pub const DEFAULT_LISTEN_ADDRESS: &str = "::";
pub const DEFAULT_DATABASE_URL: &str = "data/db.sqlite";
pub const DEFAULT_DATA_DIR: &str = "data";

/// Everything the server needs from its environment, parsed once at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub listen_address: String,
    pub database_url: String,
    pub data_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + '_ {
        let map: HashMap<&str, &str> = vars.iter().copied().collect();
        move |key| map.get(key).map(|v| (*v).to_owned())
    }

    #[test]
    fn defaults_when_nothing_is_set() {
        let cfg = Config::from_lookup(lookup(&[])).unwrap();
        assert_eq!(
            cfg,
            Config {
                port: 7008,
                listen_address: "::".to_owned(),
                database_url: "data/db.sqlite".to_owned(),
                data_dir: PathBuf::from("data"),
            }
        );
    }

    #[test]
    fn reads_every_variable() {
        let cfg = Config::from_lookup(lookup(&[
            ("PORT", "9000"),
            ("LISTEN_ADDRESS", "127.0.0.1"),
            ("DATABASE_URL", "/data/db.sqlite"),
            ("DATA_DIR", "/data"),
        ]))
        .unwrap();
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.listen_address, "127.0.0.1");
        assert_eq!(cfg.database_url, "/data/db.sqlite");
        assert_eq!(cfg.data_dir, PathBuf::from("/data"));
    }

    #[test]
    fn rejects_an_unparseable_port() {
        // Review focus 1: a typo must not silently become the default port.
        for bad in ["abc", "70000", "", "-1"] {
            let err = Config::from_lookup(lookup(&[("PORT", bad)])).unwrap_err();
            assert!(err.to_string().contains("PORT"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn empty_string_for_a_path_is_rejected() {
        let err = Config::from_lookup(lookup(&[("DATABASE_URL", "")])).unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
        let err = Config::from_lookup(lookup(&[("DATA_DIR", "")])).unwrap_err();
        assert!(err.to_string().contains("DATA_DIR"));
    }
}
```

`src/lib.rs`:

```rust
#![warn(clippy::str_to_string)]

pub mod config;
pub mod net;

use tracing_subscriber::{EnvFilter, fmt};

/// Installs the global tracing subscriber, filtered by `RUST_LOG` (default `info`).
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();
}
```

- [ ] **Step 3: Run the config tests to verify they fail**

Run: `cargo test config`
Expected: compile error, `from_lookup` not found.

- [ ] **Step 4: Implement `Config`**

Add to `src/config.rs` between the struct and the tests:

```rust
impl Config {
    /// Reads configuration from the process environment (after loading `.env` if present).
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds a config from any key lookup, so tests do not touch the real environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let port = match lookup("PORT") {
            None => DEFAULT_PORT,
            Some(raw) => raw
                .trim()
                .parse::<u16>()
                .with_context(|| format!("PORT must be a number between 1 and 65535, got {raw:?}"))?,
        };
        if port == 0 {
            bail!("PORT must be between 1 and 65535, got 0");
        }
        let listen_address = non_empty(lookup("LISTEN_ADDRESS"), "LISTEN_ADDRESS", DEFAULT_LISTEN_ADDRESS)?;
        let database_url = non_empty(lookup("DATABASE_URL"), "DATABASE_URL", DEFAULT_DATABASE_URL)?;
        let data_dir = non_empty(lookup("DATA_DIR"), "DATA_DIR", DEFAULT_DATA_DIR)?;
        Ok(Self {
            port,
            listen_address,
            database_url,
            data_dir: PathBuf::from(data_dir),
        })
    }
}

fn non_empty(value: Option<String>, name: &str, default: &str) -> Result<String> {
    match value {
        None => Ok(default.to_owned()),
        Some(v) if v.trim().is_empty() => bail!("{name} is set but empty"),
        Some(v) => Ok(v),
    }
}
```

- [ ] **Step 5: Port `net.rs` with its tests from chore-tracker**

Copy `/home/steve/src/chore-tracker/src/net.rs` verbatim to `src/net.rs`, then change every `7007` in its tests to `7008`. It has no other project-specific content.

- [ ] **Step 6: Write the CLI skeleton**

`src/main.rs`:

```rust
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
            tracing::info!(port = port.unwrap_or(config.port), "healthcheck: not implemented yet");
            Ok(())
        }
    }
}
```

- [ ] **Step 7: Build, test, lint**

Run: `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check && cargo tree -i openssl; cargo tree -i native-tls`
Expected: build OK, all `config` and `net` tests pass, clippy and fmt clean, both `cargo tree -i` calls report the package is not in the graph.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock rustfmt.toml diesel.toml .gitignore src/
git commit -m "feat: crate manifest, config, dual-stack bind, CLI skeleton

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 2: SQLite pool, embedded migrations, settings

**Files:**
- Create: `migrations/2026-09-25-000000_settings/up.sql`, `migrations/2026-09-25-000000_settings/down.sql`, `src/schema.rs`, `src/db.rs`, `src/svc/mod.rs`, `src/svc/settings.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `home_tracker::db::SqlitePool` (= `Pool<ConnectionManager<SqliteConnection>>`), `db::build_pool(url: &str) -> anyhow::Result<SqlitePool>`, `db::run_migrations(conn: &mut SqliteConnection) -> anyhow::Result<()>`, `db::TestDb { pub pool: SqlitePool }` with `TestDb::new() -> TestDb` (temp-file database with migrations applied; dropping it deletes the file).
- Produces: `home_tracker::svc::settings::currency(conn: &mut SqliteConnection) -> anyhow::Result<String>`.
- Produces: `home_tracker::schema::settings` diesel table.

- [ ] **Step 1: Write the migration and schema**

`migrations/2026-09-25-000000_settings/up.sql`:

```sql
CREATE TABLE settings (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);

INSERT INTO settings (key, value) VALUES ('currency', 'USD');
```

`migrations/2026-09-25-000000_settings/down.sql`:

```sql
DROP TABLE settings;
```

`src/schema.rs` (what `diesel print-schema` produces for the above; keep it identical so later regenerations are no-ops):

```rust
// @generated automatically by Diesel CLI.

diesel::table! {
    settings (key) {
        key -> Text,
        value -> Text,
    }
}
```

- [ ] **Step 2: Write the failing db and settings tests**

`src/db.rs`:

```rust
//! SQLite connection pool and embedded migrations.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, CustomizeConnection, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

/// Sets the per-connection PRAGMAs. `busy_timeout` goes first so a locked
/// database waits instead of failing the PRAGMAs that follow.
#[derive(Debug)]
struct ConnectionOptions {
    busy_timeout: Duration,
}

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            diesel::sql_query(format!("PRAGMA busy_timeout = {};", self.busy_timeout.as_millis()))
                .execute(conn)?;
            diesel::sql_query("PRAGMA journal_mode = WAL;").execute(conn)?;
            diesel::sql_query("PRAGMA foreign_keys = ON;").execute(conn)?;
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}

/// A throwaway database for tests: a temp directory holding a fresh SQLite file
/// with all migrations applied. The directory is removed when this is dropped.
pub struct TestDb {
    pub pool: SqlitePool,
    _dir: tempfile::TempDir,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_parent_directory_of_the_database_file() {
        // Review focus 2: a fresh Docker volume has no subdirectories yet.
        let dir = tempfile::tempdir().unwrap();
        let url = dir.path().join("nested").join("deeper").join("db.sqlite");
        let pool = build_pool(url.to_str().unwrap()).unwrap();
        assert!(pool.get().is_ok());
        assert!(url.parent().unwrap().is_dir());
    }

    #[test]
    fn migrations_run_and_are_idempotent() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        // Running again must be a no-op, not an error.
        run_migrations(&mut conn).unwrap();
        let pending: i64 = diesel::sql_query("SELECT count(*) AS n FROM settings")
            .get_result::<Count>(&mut conn)
            .unwrap()
            .n;
        assert_eq!(pending, 1);
    }

    #[test]
    fn foreign_keys_are_enforced_on_every_connection() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let on = diesel::sql_query("PRAGMA foreign_keys")
            .get_result::<ForeignKeys>(&mut conn)
            .unwrap();
        assert_eq!(on.foreign_keys, 1);
    }

    #[derive(QueryableByName)]
    struct Count {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        n: i64,
    }

    #[derive(QueryableByName)]
    struct ForeignKeys {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        foreign_keys: i32,
    }
}
```

`src/svc/mod.rs`:

```rust
//! Business logic. Each module owns one aggregate and takes a `&mut SqliteConnection`.

pub mod settings;
```

`src/svc/settings.rs`:

```rust
//! Key/value settings, seeded by migrations.

use anyhow::{Context, Result};
use diesel::prelude::*;

use crate::schema::settings;

pub const CURRENCY_KEY: &str = "currency";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;

    #[test]
    fn currency_defaults_to_usd_from_the_seed_row() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        assert_eq!(currency(&mut conn).unwrap(), "USD");
    }

    #[test]
    fn a_missing_currency_row_is_an_error_not_a_default() {
        // Review focus 3: never invent a currency; the caller decides what to do.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::delete(settings::table.filter(settings::key.eq(CURRENCY_KEY)))
            .execute(&mut conn)
            .unwrap();
        let err = currency(&mut conn).unwrap_err();
        assert!(err.to_string().contains("currency"), "{err}");
    }
}
```

Add to `src/lib.rs` module list: `pub mod db; pub mod schema; pub mod svc;`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test db:: svc::settings`
Expected: compile errors for `build_pool`, `run_migrations`, `TestDb::new`, `currency`.

- [ ] **Step 4: Implement the pool, migrations, TestDb, and currency**

Add to `src/db.rs` after `ConnectionOptions`:

```rust
/// Builds the pool for `url`, creating the file's parent directory when needed.
pub fn build_pool(url: &str) -> Result<SqlitePool> {
    if let Some(parent) = Path::new(url).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create database directory {}", parent.display()))?;
    }
    let manager = ConnectionManager::<SqliteConnection>::new(url);
    Pool::builder()
        .min_idle(Some(1))
        .max_size(8)
        .connection_customizer(Box::new(ConnectionOptions {
            busy_timeout: Duration::from_secs(5),
        }))
        .build(manager)
        .with_context(|| format!("could not open database {url}"))
}

/// Applies every pending embedded migration. Safe to call on every boot.
pub fn run_migrations(conn: &mut SqliteConnection) -> Result<()> {
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("migration failed: {e}"))?;
    Ok(())
}

impl TestDb {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir for test database");
        let url = dir.path().join("test.sqlite");
        let pool = build_pool(url.to_str().expect("utf-8 temp path")).expect("test pool");
        let mut conn = pool.get().expect("test connection");
        run_migrations(&mut conn).expect("test migrations");
        Self { pool, _dir: dir }
    }
}

impl Default for TestDb {
    fn default() -> Self {
        Self::new()
    }
}
```

Add to `src/svc/settings.rs` after the constant:

```rust
/// The ISO 4217 code used to display money. Missing row is an error: the seed
/// migration creates it, so absence means the database is damaged.
pub fn currency(conn: &mut SqliteConnection) -> Result<String> {
    settings::table
        .filter(settings::key.eq(CURRENCY_KEY))
        .select(settings::value)
        .first::<String>(conn)
        .optional()
        .context("reading the currency setting")?
        .with_context(|| format!("settings row {CURRENCY_KEY:?} is missing"))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass, clean.

- [ ] **Step 6: Commit**

```bash
git add migrations src/schema.rs src/db.rs src/svc src/lib.rs
git commit -m "feat: sqlite pool, embedded migrations, settings with seeded currency

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 3: GraphQL `summary`, router, embedded site, `serve`

**Files:**
- Create: `src/svc/stats.rs`, `src/graphql/mod.rs`, `src/graphql/context.rs`, `src/graphql/schema.rs`, `src/graphql/query.rs`, `src/api/mod.rs`, `src/api/graphql.rs`, `src/routes.rs`, `tests/graphql_summary.rs`, `tests/spa_routes.rs`
- Modify: `src/svc/mod.rs`, `src/lib.rs`, `src/main.rs`

**Interfaces:**
- Consumes: `db::{SqlitePool, build_pool, run_migrations, TestDb}`, `svc::settings::currency`, `config::Config`, `net::{listen_addr, bind}`.
- Produces: `svc::stats::Summary { total_value_cents: i32, currency: String, total_items: i32, total_locations: i32, total_tags: i32 }` (derives `juniper::GraphQLObject`), `svc::stats::summary(conn: &mut SqliteConnection) -> anyhow::Result<Summary>`.
- Produces: `graphql::context::{GraphQLContext { pool: SqlitePool, actor: Actor }, Actor, Role}`, `GraphQLContext::new(pool, actor)`, `GraphQLContext::require_write(&self) -> FieldResult<()>`, `Actor::can_write(&self) -> bool`.
- Produces: `graphql::schema::{Schema, create_schema() -> Schema, graphql_translate_anyhow}`; `graphql::query::Query`.
- Produces: `routes::app(pool: SqlitePool) -> axum::Router`.
- Produces: `api::AppError` (anyhow wrapper → 500).

- [ ] **Step 1: Write the failing stats test**

`src/svc/stats.rs`:

```rust
//! Home-page statistics. Phase 1 returns fixed numbers; phase 2 computes them.

use anyhow::Result;
use diesel::prelude::*;
use juniper::GraphQLObject;

use crate::svc::settings;

/// The four quick statistics on the home page plus the currency to format value in.
#[derive(Debug, Clone, PartialEq, Eq, GraphQLObject)]
pub struct Summary {
    pub total_value_cents: i32,
    pub currency: String,
    pub total_items: i32,
    pub total_locations: i32,
    pub total_tags: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::schema::settings as settings_table;

    #[test]
    fn summary_reads_the_currency_from_settings() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::update(settings_table::table.filter(settings_table::key.eq("currency")))
            .set(settings_table::value.eq("EUR"))
            .execute(&mut conn)
            .unwrap();
        let s = summary(&mut conn).unwrap();
        assert_eq!(s.currency, "EUR");
    }

    #[test]
    fn summary_has_the_phase_one_placeholder_numbers() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let s = summary(&mut conn).unwrap();
        assert_eq!(s.total_value_cents, 1_234_567);
        assert_eq!(s.total_items, 42);
        assert_eq!(s.total_locations, 7);
        assert_eq!(s.total_tags, 5);
    }

    #[test]
    fn summary_fails_when_the_currency_row_is_gone() {
        // Review focus 3.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::delete(settings_table::table).execute(&mut conn).unwrap();
        assert!(summary(&mut conn).is_err());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test svc::stats`
Expected: compile error, `summary` not found.

- [ ] **Step 3: Implement `summary`**

Add to `src/svc/stats.rs` after the struct:

```rust
/// Phase 1 placeholder: fixed counts, real currency. Phase 2 replaces the
/// constants with the queries in the spec (§3 rows 7 and 8).
pub fn summary(conn: &mut SqliteConnection) -> Result<Summary> {
    let currency = settings::currency(conn)?;
    Ok(Summary {
        total_value_cents: 1_234_567,
        currency,
        total_items: 42,
        total_locations: 7,
        total_tags: 5,
    })
}
```

Add `pub mod stats;` to `src/svc/mod.rs`. Run `cargo test svc::stats` → 3 pass.

- [ ] **Step 4: Write the GraphQL context, schema, and query**

`src/graphql/mod.rs`:

```rust
pub mod context;
pub mod query;
pub mod schema;
```

`src/graphql/context.rs`:

```rust
//! Per-request GraphQL context and the authorization seam.

use juniper::{FieldError, FieldResult};

use crate::db::SqlitePool;

/// What a user may do. v1 has no users; the enum exists so mutations gate on it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    ReadOnly,
    Write,
}

/// Who is making the request. Auth will replace `Anonymous` with `User`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// No authentication configured. Has full access in v1.
    Anonymous,
    User { id: String, role: Role },
}

impl Actor {
    pub fn can_write(&self) -> bool {
        match self {
            Self::Anonymous => true,
            Self::User { role, .. } => *role == Role::Write,
        }
    }
}

#[derive(Clone)]
pub struct GraphQLContext {
    pub pool: SqlitePool,
    pub actor: Actor,
}

impl juniper::Context for GraphQLContext {}

impl GraphQLContext {
    pub fn new(pool: SqlitePool, actor: Actor) -> Self {
        Self { pool, actor }
    }

    /// The single gate every mutation calls first.
    pub fn require_write(&self) -> FieldResult<()> {
        if self.actor.can_write() {
            Ok(())
        } else {
            Err(FieldError::new("Forbidden: write access required", juniper::Value::null()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;

    #[test]
    fn anonymous_can_write_in_v1() {
        assert!(Actor::Anonymous.can_write());
    }

    #[test]
    fn read_only_users_are_refused() {
        let db = TestDb::new();
        let ctx = GraphQLContext::new(
            db.pool.clone(),
            Actor::User { id: "u1".to_owned(), role: Role::ReadOnly },
        );
        assert!(ctx.require_write().is_err());
        let ctx = GraphQLContext::new(
            db.pool,
            Actor::User { id: "u1".to_owned(), role: Role::Write },
        );
        assert!(ctx.require_write().is_ok());
    }
}
```

`src/graphql/schema.rs`:

```rust
use juniper::{EmptyMutation, EmptySubscription, FieldError, FieldResult, RootNode};
use tracing::error;

use super::context::GraphQLContext;
use super::query::Query;

pub type Schema =
    RootNode<Query, EmptyMutation<GraphQLContext>, EmptySubscription<GraphQLContext>>;

pub fn create_schema() -> Schema {
    Schema::new(Query, EmptyMutation::new(), EmptySubscription::new())
}

/// Converts an `anyhow::Result` into a juniper `FieldResult`, logging failures.
pub fn graphql_translate_anyhow<T>(res: anyhow::Result<T>) -> FieldResult<T> {
    res.map_err(|e| {
        error!("GraphQL error: {e:#}");
        FieldError::from(e)
    })
}
```

`src/graphql/query.rs`:

```rust
use anyhow::Context as _;
use juniper::FieldResult;

use super::context::GraphQLContext;
use super::schema::graphql_translate_anyhow;
use crate::svc::stats::{self, Summary};

pub struct Query;

#[juniper::graphql_object(context = GraphQLContext)]
impl Query {
    /// The home-page quick statistics.
    fn summary(context: &GraphQLContext) -> FieldResult<Summary> {
        graphql_translate_anyhow(
            context
                .pool
                .get()
                .context("could not get a database connection")
                .and_then(|mut conn| stats::summary(&mut conn)),
        )
    }
}
```

- [ ] **Step 5: Write the axum layer**

`src/api/mod.rs`:

```rust
//! Plain HTTP handlers (everything that is not GraphQL).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub mod graphql;

/// `anyhow::Error` → 500 with the message. Handlers return `Result<_, AppError>` and use `?`.
pub struct AppError(pub anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {:#}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", self.0)).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
```

`src/api/graphql.rs`:

```rust
use std::sync::Arc;

use axum::routing::{MethodFilter, get, on};
use axum::{Extension, Router};
use juniper_axum::extract::JuniperRequest;
use juniper_axum::graphiql;
use juniper_axum::response::JuniperResponse;

use crate::db::SqlitePool;
use crate::graphql::context::{Actor, GraphQLContext};
use crate::graphql::schema::Schema;

/// `/graphql` (GET and POST) plus GraphiQL at `/graphiql` in debug builds.
pub fn graphql_routes(pool: SqlitePool, schema: Arc<Schema>) -> Router {
    let router = Router::new().route("/graphql", on(MethodFilter::GET.or(MethodFilter::POST), handle));
    let router = if cfg!(debug_assertions) {
        router.route("/graphiql", get(graphiql("/graphql", None)))
    } else {
        router
    };
    router.layer(Extension(pool)).layer(Extension(schema))
}

async fn handle(
    Extension(schema): Extension<Arc<Schema>>,
    Extension(pool): Extension<SqlitePool>,
    JuniperRequest(request): JuniperRequest,
) -> JuniperResponse {
    // A fresh context per request: auth will fill `actor` from the request here.
    let context = GraphQLContext::new(pool, Actor::Anonymous);
    JuniperResponse(request.execute(&*schema, &context).await)
}
```

`src/routes.rs`:

```rust
//! Top-level router: GraphQL, static assets, SPA fallback, compression.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;
use tower_http::compression::CompressionLayer;

use crate::api::graphql::graphql_routes;
use crate::db::SqlitePool;
use crate::graphql::schema::create_schema;

/// The Vite build output. `site/build` must exist at compile time (see `just site-placeholder`).
#[derive(RustEmbed, Clone)]
#[folder = "site/build/"]
struct Assets;

fn serve_asset(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

async fn static_handler(uri: Uri) -> Response {
    serve_asset(uri.path().trim_start_matches('/'))
}

/// Every non-asset, non-API path serves the SPA shell so client routing can take over.
async fn index_handler() -> Response {
    serve_asset("index.html")
}

async fn immutable_cache(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
}

pub fn app(pool: SqlitePool) -> Router {
    let schema = Arc::new(create_schema());
    Router::new()
        .route("/assets/{*path}", get(static_handler))
        .layer(middleware::from_fn(immutable_cache))
        .merge(graphql_routes(pool, schema))
        .route("/", get(index_handler))
        .fallback(get(index_handler))
        .layer(CompressionLayer::new())
}
```

Add `pub mod api; pub mod graphql; pub mod routes;` to `src/lib.rs`.

- [ ] **Step 6: Write the failing integration tests**

`tests/graphql_summary.rs`:

```rust
use axum_test::TestServer;
use home_tracker::db::TestDb;
use home_tracker::routes::app;
use serde_json::{Value, json};

#[tokio::test]
async fn summary_query_returns_the_placeholder_numbers_and_currency() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone())).unwrap();

    let response = server
        .post("/graphql")
        .json(&json!({
            "query": "{ summary { totalValueCents currency totalItems totalLocations totalTags } }"
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(
        body,
        json!({
            "data": {
                "summary": {
                    "totalValueCents": 1_234_567,
                    "currency": "USD",
                    "totalItems": 42,
                    "totalLocations": 7,
                    "totalTags": 5
                }
            }
        })
    );
}

#[tokio::test]
async fn unknown_field_is_a_graphql_error_not_a_500() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone())).unwrap();

    let response = server
        .post("/graphql")
        .json(&json!({ "query": "{ nope }" }))
        .await;

    response.assert_status_bad_request();
    let body: Value = response.json();
    assert!(body["errors"].is_array());
}
```

`tests/spa_routes.rs`:

```rust
use axum_test::TestServer;
use home_tracker::db::TestDb;
use home_tracker::routes::app;

// `site/build/index.html` must exist at compile time; `just site-placeholder` creates it.

#[tokio::test]
async fn root_serves_the_spa_shell_as_html() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone())).unwrap();
    let response = server.get("/").await;
    response.assert_status_ok();
    assert!(response.header("content-type").to_str().unwrap().starts_with("text/html"));
}

#[tokio::test]
async fn deep_links_fall_back_to_the_spa_shell() {
    // Review focus 4: a reload on /locations/abc must boot the SPA, not 404.
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone())).unwrap();
    let shell = server.get("/").await;
    let deep = server.get("/locations/abc").await;
    deep.assert_status_ok();
    assert!(deep.header("content-type").to_str().unwrap().starts_with("text/html"));
    assert_eq!(deep.text(), shell.text());
}

#[tokio::test]
async fn missing_assets_are_404_not_the_shell() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone())).unwrap();
    server.get("/assets/does-not-exist.js").await.assert_status_not_found();
}
```

- [ ] **Step 7: Create the site placeholder and run the tests**

Run: `mkdir -p site/build && [ -f site/build/index.html ] || echo '<!doctype html><title>home-tracker</title>' > site/build/index.html && cargo test`
Expected: everything passes, including both integration files. If juniper returns 200 for the invalid query instead of 400, change the assertion to `response.assert_status_ok()` and keep the `errors` array check; juniper_axum's status for validation errors is what it is, the invariant is "no 500".

- [ ] **Step 8: Wire `serve` in `main.rs`**

Replace the `Command::Serve` arm:

```rust
        Command::Serve => serve(config).await,
```

and add below `main`:

```rust
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
```

- [ ] **Step 9: Smoke-test the server**

Run in one shell: `cargo run` (expect the "listening" log on `[::]:7008`). In another:

```bash
curl -s -X POST localhost:7008/graphql -H 'content-type: application/json' \
  -d '{"query":"{ summary { totalItems currency } }"}'
```

Expected: `{"data":{"summary":{"totalItems":42,"currency":"USD"}}}`. Then `curl -s localhost:7008/locations/x | head -c 40` shows the placeholder HTML. Stop the server with Ctrl-C and confirm it exits with the "shutting down" log. Remove the created `data/` directory afterwards (`rm -rf data`); it is git-ignored anyway.

- [ ] **Step 10: Lint and commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`

```bash
git add src tests
git commit -m "feat: graphql summary query, router with embedded SPA, serve command

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 4: Healthcheck subcommand and import stub

**Files:**
- Create: `src/healthcheck.rs`, `tests/healthcheck.rs`
- Modify: `src/lib.rs`, `src/main.rs`

**Interfaces:**
- Consumes: `config::Config`.
- Produces: `home_tracker::healthcheck::probe(addr: SocketAddr, timeout: Duration) -> io::Result<()>`, `healthcheck::probe_local(port: u16, timeout: Duration) -> io::Result<()>`, `healthcheck::run(port: u16) -> !`.

- [ ] **Step 1: Write the failing probe tests**

`tests/healthcheck.rs`:

```rust
use home_tracker::healthcheck::probe;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread;
use std::time::Duration;

#[test]
fn probe_succeeds_against_a_live_listener() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = [0u8; 64];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
        }
    });
    // Any HTTP answer, even a 404, proves the process is up.
    assert!(probe(addr, Duration::from_secs(3)).is_ok());
    handle.join().unwrap();
}

#[test]
fn probe_fails_against_a_closed_port() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    drop(listener);
    assert!(probe(addr, Duration::from_millis(500)).is_err());
}

#[test]
fn probe_fails_when_the_server_closes_without_answering() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        if let Ok((sock, _)) = listener.accept() {
            drop(sock);
        }
    });
    assert!(probe(addr, Duration::from_secs(3)).is_err());
    handle.join().unwrap();
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --test healthcheck`
Expected: compile error, module `healthcheck` not found.

- [ ] **Step 3: Implement the probe**

`src/healthcheck.rs`:

```rust
//! Liveness probe for the Docker `HEALTHCHECK`. The `scratch` image has no shell
//! or curl, so the check execs this binary with the `healthcheck` subcommand.

use std::io::{self, Read, Write};
use std::net::{Ipv6Addr, SocketAddr, TcpStream};
use std::process;
use std::time::Duration;

fn loopback_v4(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn loopback_v6(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv6Addr::LOCALHOST, port))
}

/// Only "nothing listens on this family" errors are worth retrying on the other
/// loopback. A timeout has already spent the budget; retrying would get the
/// Docker check killed instead of reported.
fn worth_retrying(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::AddrNotAvailable
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::HostUnreachable
    )
}

/// Probe `127.0.0.1:port`, then `[::1]:port` if IPv4 is not listening at all.
pub fn probe_local(port: u16, timeout: Duration) -> io::Result<()> {
    match probe(loopback_v4(port), timeout) {
        Err(e) if worth_retrying(&e) => probe(loopback_v6(port), timeout),
        other => other,
    }
}

/// Connect, send a minimal request, and require at least one byte back.
pub fn probe(addr: SocketAddr, timeout: Duration) -> io::Result<()> {
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "empty response from server"));
    }
    Ok(())
}

/// Exit 0 when healthy, 1 otherwise.
pub fn run(port: u16) -> ! {
    match probe_local(port, Duration::from_secs(3)) {
        Ok(()) => process::exit(0),
        Err(e) => {
            eprintln!("healthcheck failed: {e}");
            process::exit(1);
        }
    }
}
```

Add `pub mod healthcheck;` to `src/lib.rs`.

- [ ] **Step 4: Wire the subcommands in `main.rs`**

Replace the `Healthcheck` and `Import` arms:

```rust
        Command::Import { path } => {
            anyhow::bail!(
                "import is not implemented yet (planned for phase 2); got {}",
                path.display()
            )
        }
        Command::Healthcheck { port } => home_tracker::healthcheck::run(port.unwrap_or(config.port)),
```

Because `run` returns `!`, the match arm type-checks against `Result<()>`.

- [ ] **Step 5: Run everything**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: pass. Then manual: `cargo run -- healthcheck; echo exit=$?` → prints `healthcheck failed: ...` and `exit=1` when no server runs; with `cargo run` in another shell it prints nothing and `exit=0`. And `cargo run -- import /tmp/x; echo exit=$?` → the "not implemented" error and `exit=1`.

- [ ] **Step 6: Commit**

```bash
git add src tests
git commit -m "feat: healthcheck subcommand and import stub

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 5: Docker image, justfile, CI workflow

**Files:**
- Create: `Dockerfile`, `.dockerignore`, `env.prod`, `justfile`, `.github/workflows/rust.yml`

**Interfaces:**
- Consumes: the `healthcheck` subcommand, `PORT` default 7008, `site/build` embedding.
- Produces: `just site-placeholder`, `just test`, `just cover`, `just import <path>`, `just build-site`.

- [ ] **Step 1: Write the Dockerfile and its companions**

`Dockerfile`:

```dockerfile
# Stage 1: warm the musl dependency cache with a stub crate so source edits
# don't rebuild every dependency.
FROM docker.io/blackdex/rust-musl:x86_64-musl AS dependencybuilder
WORKDIR /home/rust/src
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src site/build \
  && echo "fn main() {}" > src/main.rs \
  && echo "" > src/lib.rs \
  && echo "<!doctype html>" > site/build/index.html
RUN cargo build --release
RUN rm -rf src

# Stage 2: the real build. site/build is produced by CI's build-site job (or
# `just build-site` locally) before this runs.
FROM dependencybuilder AS builder
COPY src ./src/
COPY migrations ./migrations/
COPY site/build ./site/build/
RUN find src -name '*.rs' -exec touch {} + && cargo build --release

# Stage 3: static binary only.
FROM scratch
WORKDIR /
COPY --from=builder /home/rust/src/target/x86_64-unknown-linux-musl/release/home-tracker /home-tracker
COPY env.prod /.env

VOLUME ["/data"]
EXPOSE 7008

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD ["/home-tracker", "healthcheck"]

CMD ["/home-tracker"]
```

`.dockerignore`:

```
target
site/node_modules
site/src
site/public
site/coverage
data
db
docs
homebox
homebox-backup
.git
.github
.superpowers
```

`env.prod`:

```
DATABASE_URL=/data/db.sqlite
DATA_DIR=/data
PORT=7008
LISTEN_ADDRESS=::
```

`justfile`:

```just
# Create the empty site bundle the Rust build embeds when the real site hasn't been built.
site-placeholder:
    mkdir -p site/build
    [ -f site/build/index.html ] || echo '<!doctype html><title>home-tracker</title>' > site/build/index.html

# Build the React site into site/build (what the release binary embeds).
build-site:
    cd site && yarn install --frozen-lockfile && yarn build

test: site-placeholder
    watchexec -e rs,toml,sql cargo test

cover: site-placeholder
    cargo llvm-cov --lcov --output-path lcov.info

# Import a Homebox backup zip or exploded directory into the dev database.
import path:
    cargo run -- import {{path}}

docker-build:
    docker build -t home-tracker:dev .
```

- [ ] **Step 2: Write the CI workflow**

`.github/workflows/rust.yml` is chore-tracker's `/home/steve/src/chore-tracker/.github/workflows/rust.yml` with these substitutions and nothing else: every `chore-tracker` → `home-tracker`; in the `coverage` job keep the `mkdir -p site/build && touch site/build/index.html` step; drop the commented-out `test-site` block; add a `test-site` job (uncommented) that runs `yarn install --frozen-lockfile` then `yarn test --run` in `./site` using the same `borales/actions-yarn@v5` steps as `build-site`, and make `build-docker` `needs: [build-site, test-site]`. Keep the Tailscale, buildx `network=host`, registry cache and Watchtower steps and comments verbatim; the secrets and variables carry the same names.

- [ ] **Step 3: Validate the workflow file and build the image**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/rust.yml')); print('yaml ok')"` (if PyYAML is missing, `ruby -ryaml -e 'YAML.load_file(".github/workflows/rust.yml"); puts "yaml ok"'`, and if neither exists, `docker run --rm -v "$PWD:/w" -w /w mikefarah/yq '.jobs | keys' .github/workflows/rust.yml`).
Expected: parses; jobs are `coverage`, `build-site`, `test-site`, `build-docker`.

Run: `just site-placeholder && docker build -t home-tracker:dev .` (allow up to 20 minutes on a cold cache).
Expected: image builds. Then:

```bash
docker run -d --name ht-smoke -p 17008:7008 home-tracker:dev
sleep 3
curl -s -X POST localhost:17008/graphql -H 'content-type: application/json' -d '{"query":"{ summary { currency } }"}'
docker exec ht-smoke /home-tracker healthcheck; echo "healthcheck exit=$?"
docker rm -f ht-smoke
```

Expected: `{"data":{"summary":{"currency":"USD"}}}` and `healthcheck exit=0`.

- [ ] **Step 4: Commit**

```bash
git add Dockerfile .dockerignore env.prod justfile .github
git commit -m "build: musl/scratch Dockerfile with healthcheck, justfile, CI workflow

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 6: Frontend scaffold, theme tokens, ThemeProvider

**Files:**
- Create: `package.json` (root), `.husky/pre-commit`, `site/package.json`, `site/vite.config.ts`, `site/tsconfig.json`, `site/tsconfig.node.json`, `site/eslint.config.js`, `site/.prettierrc`, `site/.lintstagedrc.json`, `site/index.html`, `site/setupVitest.ts`, `site/src/main.tsx`, `site/src/vite-env.d.ts`, `site/src/index.css`, `site/src/theme/tokens.css`, `site/src/theme/ThemeProvider.tsx`, `site/src/theme/useTheme.ts`, `site/src/theme/__tests__/useTheme.test.tsx`, `site/src/App.tsx` (minimal, replaced in Task 7)

**Interfaces:**
- Produces: `ThemeProvider` (React context provider), `useTheme(): { theme: Theme; setTheme: (t: Theme) => void; toggle: () => void }`, `type Theme = 'dark' | 'light'`, `THEME_STORAGE_KEY = 'home-tracker.theme'`.
- Produces: semantic Tailwind colour utilities `bg-bg`, `bg-surface`, `bg-surface-raised`, `border-border`, `text-text`, `text-muted`, `bg-accent`, `text-accent`, `text-accent-text`, `text-danger`, `text-success`.

- [ ] **Step 1: Write the package manifests and tool configs**

Root `package.json`:

```json
{
  "private": true,
  "devDependencies": {
    "husky": "^9.1.7",
    "lint-staged": "^15.2.4"
  },
  "scripts": {
    "prepare": "husky"
  }
}
```

`.husky/pre-commit`:

```sh
cd site && npx lint-staged && CI=true yarn test --run
cd .. && cargo fmt --all --check && cargo clippy --all-targets -- -D warnings
```

`site/package.json`:

```json
{
  "name": "site",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "lint": "eslint . --ext ts,tsx --report-unused-disable-directives --max-warnings 0",
    "preview": "vite preview",
    "test": "vitest"
  },
  "dependencies": {
    "@apollo/client": "^4.2.0",
    "@tailwindcss/vite": "^4.1.16",
    "clsx": "^2.1.0",
    "graphql": "^16.11.0",
    "react": "^19.2.0",
    "react-dom": "^19.2.0",
    "react-router-dom": "^7.9.4",
    "react-toastify": "^11.0.5",
    "tailwindcss": "^4.1.16",
    "tslib": "^2.8.1",
    "vite-tsconfig-paths": "^6.1.1"
  },
  "devDependencies": {
    "@eslint/js": "^10.0.1",
    "@testing-library/dom": "^10.4.1",
    "@testing-library/jest-dom": "^6.9.1",
    "@testing-library/react": "^16.3.0",
    "@testing-library/user-event": "^14.6.1",
    "@types/node": "^25.3.3",
    "@types/react": "^19.2.2",
    "@types/react-dom": "^19.2.2",
    "@vitejs/plugin-react": "^6.0.2",
    "@vitest/coverage-v8": "^4.0.3",
    "eslint": "^10.0.2",
    "eslint-config-prettier": "^10.1.8",
    "eslint-plugin-jsx-a11y": "^6.10.2",
    "eslint-plugin-prettier": "^5.5.4",
    "eslint-plugin-react-hooks": "^7.0.1",
    "eslint-plugin-react-refresh": "^0.5.2",
    "globals": "^17.4.0",
    "jsdom": "^29.1.1",
    "prettier": "^3.6.2",
    "typescript": "^6.0.3",
    "typescript-eslint": "^8.46.2",
    "vite": "^8.0.14",
    "vite-plugin-compression2": "^2.3.0",
    "vite-plugin-eslint": "^1.8.1",
    "vitest": "^4.0.3",
    "vitest-fetch-mock": "^0.4.5"
  }
}
```

`site/vite.config.ts`:

```ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import eslint from 'vite-plugin-eslint';
import viteTsconfigPaths from 'vite-tsconfig-paths';
import compress from 'vite-plugin-compression2';
import tailwindcss from '@tailwindcss/vite';

const PROXY_ENDPOINT = 'http://localhost:7008';
const proxied = { target: PROXY_ENDPOINT, changeOrigin: true, secure: false };

export default defineConfig(() => ({
  build: { outDir: 'build' },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./setupVitest.ts'],
    coverage: {
      reporter: ['text', 'html', 'cobertura', 'lcov', 'json-summary'],
      exclude: ['**/node_modules/**', '**/build/**', '**/*.js', 'src/main.tsx'],
    },
  },
  plugins: [react(), eslint(), viteTsconfigPaths(), tailwindcss(), compress()],
  server: {
    watch: { ignored: ['coverage', 'build'] },
    proxy: {
      '/graphql': proxied,
      '/graphiql': proxied,
      '/attachments/': proxied,
      '/api/': proxied,
    },
  },
}));
```

`site/tsconfig.json`:

```json
{
  "compilerOptions": {
    "baseUrl": "src",
    "outDir": "build/dist",
    "module": "esnext",
    "target": "ESNext",
    "lib": ["dom", "dom.iterable", "esnext"],
    "sourceMap": true,
    "allowJs": false,
    "jsx": "react-jsx",
    "importHelpers": true,
    "moduleResolution": "bundler",
    "rootDir": "src",
    "forceConsistentCasingInFileNames": true,
    "noImplicitReturns": true,
    "noImplicitThis": true,
    "strictNullChecks": true,
    "noUnusedLocals": true,
    "types": ["vitest/globals"],
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true
  },
  "exclude": ["node_modules", "build"],
  "include": ["src"]
}
```

`site/tsconfig.node.json`:

```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "strict": true
  },
  "include": ["vite.config.ts"]
}
```

`site/eslint.config.js`: copy `/home/steve/src/chore-tracker/site/eslint.config.js` verbatim and delete the `storybook-static/**` and `.storybook/**` ignore entries.

`site/.prettierrc`:

```json
{ "singleQuote": true, "printWidth": 100 }
```

`site/.lintstagedrc.json`:

```json
{
  "*.{ts,tsx}": ["eslint --fix"],
  "*.{css,html,json,md}": ["prettier --write"]
}
```

`site/index.html`:

```html
<!doctype html>
<html lang="en" data-theme="dark">
  <head>
    <meta charset="UTF-8" />
    <meta name="description" content="Home Tracker" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Home Tracker</title>
  </head>
  <body class="bg-bg text-text">
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`site/setupVitest.ts`:

```ts
import '@testing-library/jest-dom/vitest';
import createFetchMock from 'vitest-fetch-mock';
import { vi, beforeEach } from 'vitest';

const fetchMocker = createFetchMock(vi);
fetchMocker.enableMocks();

beforeEach(() => {
  fetchMocker.resetMocks();
  // Default GraphQL answer so App-level tests can render without Apollo mocks.
  fetchMocker.mockIf(/\/graphql$/, async () => ({
    body: JSON.stringify({
      data: {
        summary: {
          totalValueCents: 1234567,
          currency: 'USD',
          totalItems: 42,
          totalLocations: 7,
          totalTags: 5,
        },
      },
    }),
    headers: { 'content-type': 'application/json' },
  }));
});
```

`site/src/vite-env.d.ts`:

```ts
/// <reference types="vite/client" />
```

`site/src/main.tsx`:

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from 'App';
import 'index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

`site/src/App.tsx` (temporary, Task 7 replaces it):

```tsx
const App = () => <h1>Home Tracker</h1>;
export default App;
```

- [ ] **Step 2: Write the theme tokens and CSS**

`site/src/theme/tokens.css`:

```css
/*
 * Semantic colour tokens. Components use only the Tailwind utilities derived
 * from these (bg-surface, text-muted, ...), never raw palette classes, so a
 * theme is exactly one block of variables. Dark is the default; the light
 * block is a stub to be tuned when a light theme is actually wanted.
 */
:root,
:root[data-theme='dark'] {
  color-scheme: dark;
  --t-bg: #0f1115;
  --t-surface: #171a21;
  --t-surface-raised: #1f2330;
  --t-border: #2a2f3d;
  --t-text: #e6e8ee;
  --t-text-muted: #9aa3b5;
  --t-accent: #6ea8fe;
  --t-accent-text: #0b1020;
  --t-danger: #f0616d;
  --t-success: #4fd1a1;
}

:root[data-theme='light'] {
  color-scheme: light;
  --t-bg: #f6f7fb;
  --t-surface: #ffffff;
  --t-surface-raised: #eef0f6;
  --t-border: #d6dae6;
  --t-text: #171a21;
  --t-text-muted: #5b6474;
  --t-accent: #2f6fed;
  --t-accent-text: #ffffff;
  --t-danger: #c92a37;
  --t-success: #1f8f66;
}

@theme inline {
  --color-bg: var(--t-bg);
  --color-surface: var(--t-surface);
  --color-surface-raised: var(--t-surface-raised);
  --color-border: var(--t-border);
  --color-text: var(--t-text);
  --color-muted: var(--t-text-muted);
  --color-accent: var(--t-accent);
  --color-accent-text: var(--t-accent-text);
  --color-danger: var(--t-danger);
  --color-success: var(--t-success);
}
```

`site/src/index.css`:

```css
@import 'tailwindcss';
@import './theme/tokens.css';

@layer base {
  /* Tailwind v4 Preflight resets buttons to cursor: default; restore it once here. */
  button:not(:disabled),
  [role='button']:not(:disabled) {
    cursor: pointer;
  }

  h1 {
    @apply text-3xl font-semibold;
  }
  h2 {
    @apply text-2xl font-semibold;
  }
  h3 {
    @apply text-xl font-semibold;
  }
}

#root {
  min-height: 100vh;
}
```

- [ ] **Step 3: Write the failing theme tests**

`site/src/theme/__tests__/useTheme.test.tsx`:

```tsx
import { describe, it, expect, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import React from 'react';
import { ThemeProvider } from '../ThemeProvider';
import { THEME_STORAGE_KEY, useTheme } from '../useTheme';

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <ThemeProvider>{children}</ThemeProvider>
);

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
});

describe('useTheme', () => {
  it('defaults to dark and stamps the html element', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
  });

  it('restores a stored theme', () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'light');
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');
  });

  it('ignores garbage in storage and falls back to dark', () => {
    // Review focus 5.
    localStorage.setItem(THEME_STORAGE_KEY, 'neon');
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.theme).toBe('dark');
  });

  it('survives storage that throws', () => {
    const original = Storage.prototype.getItem;
    Storage.prototype.getItem = () => {
      throw new Error('private mode');
    };
    try {
      const { result } = renderHook(() => useTheme(), { wrapper });
      expect(result.current.theme).toBe('dark');
    } finally {
      Storage.prototype.getItem = original;
    }
  });

  it('toggle flips the theme and persists it', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });
    act(() => result.current.toggle());
    expect(result.current.theme).toBe('light');
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('light');
    expect(document.documentElement.dataset.theme).toBe('light');
    act(() => result.current.toggle());
    expect(result.current.theme).toBe('dark');
  });

  it('throws outside a ThemeProvider so misuse is loud', () => {
    expect(() => renderHook(() => useTheme())).toThrow(/ThemeProvider/);
  });
});
```

- [ ] **Step 4: Install and run to verify the tests fail**

Run: `yarn install` (repo root, activates husky) then `cd site && yarn install && yarn test --run`
Expected: the theme test file fails on missing modules.

- [ ] **Step 5: Implement the theme**

`site/src/theme/useTheme.ts`:

```ts
import { createContext, useContext } from 'react';

export type Theme = 'dark' | 'light';
export const THEMES: readonly Theme[] = ['dark', 'light'];
export const DEFAULT_THEME: Theme = 'dark';
export const THEME_STORAGE_KEY = 'home-tracker.theme';

export interface ThemeContextValue {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  toggle: () => void;
}

export const ThemeContext = createContext<ThemeContextValue | null>(null);

export function isTheme(value: unknown): value is Theme {
  return typeof value === 'string' && (THEMES as readonly string[]).includes(value);
}

/** Reads the stored theme, tolerating missing, invalid, or throwing storage. */
export function readStoredTheme(): Theme {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    return isTheme(stored) ? stored : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error('useTheme must be used inside a ThemeProvider');
  return value;
}
```

`site/src/theme/ThemeProvider.tsx`:

```tsx
import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  THEME_STORAGE_KEY,
  Theme,
  ThemeContext,
  ThemeContextValue,
  readStoredTheme,
} from './useTheme';

export const ThemeProvider = ({ children }: { children: React.ReactNode }) => {
  const [theme, setThemeState] = useState<Theme>(readStoredTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
    try {
      localStorage.setItem(THEME_STORAGE_KEY, next);
    } catch {
      // Storage is a convenience; the in-memory state still applies.
    }
  }, []);

  const value = useMemo<ThemeContextValue>(
    () => ({
      theme,
      setTheme,
      toggle: () => setTheme(theme === 'dark' ? 'light' : 'dark'),
    }),
    [theme, setTheme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
};
```

- [ ] **Step 6: Run tests, lint, and build**

Run in `site/`: `yarn test --run && yarn lint && yarn build`
Expected: 6 theme tests pass, lint clean, `site/build/index.html` produced. If `yarn lint` complains about `react-refresh/only-export-components` in `useTheme.ts`, that file exports no components, so the warning should not fire; if it fires on `ThemeProvider.tsx`, keep the component as the only export there (it already is).

- [ ] **Step 7: Commit**

```bash
git add package.json yarn.lock .husky site/package.json site/yarn.lock site/vite.config.ts site/tsconfig.json site/tsconfig.node.json site/eslint.config.js site/.prettierrc site/.lintstagedrc.json site/index.html site/setupVitest.ts site/src
git commit -m "feat(site): vite/react/tailwind scaffold with semantic theme tokens

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

If the husky hook fails on `cargo clippy` because `site/build` was just regenerated, that is unrelated to this task; rerun the commit.

---

### Task 7: Summary hook, stat cards, home page, layout, routing

**Files:**
- Create: `site/src/types/summary.ts`, `site/src/hooks/queries.ts`, `site/src/hooks/useErrorToast.ts`, `site/src/hooks/useSummary.ts`, `site/src/hooks/__tests__/useSummary.test.tsx`, `site/src/utils/currency.ts`, `site/src/utils/__tests__/currency.test.ts`, `site/src/components/StatCard.tsx`, `site/src/components/__tests__/StatCard.test.tsx`, `site/src/components/AppHeader.tsx`, `site/src/components/Sidebar.tsx`, `site/src/page/PageTemplate.tsx`, `site/src/page/HomePage.tsx`, `site/src/page/__tests__/HomePage.test.tsx`, `site/src/App.test.tsx`
- Modify: `site/src/App.tsx`

**Interfaces:**
- Consumes: `ThemeProvider`, `useTheme`.
- Produces: `Summary` TS type, `GET_SUMMARY` document, `useSummary(): { summary: Summary | null; loading: boolean }`, `formatCents(cents: number, currency: string, locale?: string): string`, `<StatCard label value />`, `<AppHeader />`, `<Sidebar />`, `<PageTemplate />`, `<HomePage />`.

- [ ] **Step 1: Write the failing tests**

`site/src/utils/__tests__/currency.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { formatCents } from '../currency';

describe('formatCents', () => {
  it('formats whole and fractional dollars', () => {
    expect(formatCents(1234567, 'USD', 'en-US')).toBe('$12,345.67');
    expect(formatCents(0, 'USD', 'en-US')).toBe('$0.00');
  });

  it('respects the currency code', () => {
    expect(formatCents(1999, 'EUR', 'en-US')).toBe('€19.99');
  });

  it('never throws on an unknown currency code', () => {
    expect(formatCents(100, 'NOPE', 'en-US')).toBe('1.00 NOPE');
  });
});
```

`site/src/hooks/__tests__/useSummary.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useSummary } from '../useSummary';
import { GET_SUMMARY } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const summary = {
  totalValueCents: 1234567,
  currency: 'USD',
  totalItems: 42,
  totalLocations: 7,
  totalTags: 5,
};

beforeEach(() => vi.clearAllMocks());

describe('useSummary', () => {
  it('exposes the summary once loaded', async () => {
    const mocks = [{ request: { query: GET_SUMMARY }, result: { data: { summary } } }];
    const { result } = renderHook(() => useSummary(), { wrapper: wrapper(mocks) });
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.summary).toEqual(summary);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns null on error', async () => {
    const mocks = [{ request: { query: GET_SUMMARY }, error: new Error('boom') }];
    const { result } = renderHook(() => useSummary(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.summary).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Error loading summary');
  });
});
```

`site/src/components/__tests__/StatCard.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatCard } from '../StatCard';

describe('StatCard', () => {
  it('renders the label and value', () => {
    render(<StatCard label="Total Items" value="42" />);
    expect(screen.getByText('Total Items')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
  });

  it('shows a placeholder while loading', () => {
    render(<StatCard label="Total Items" value={null} />);
    expect(screen.getByLabelText('Total Items loading')).toBeInTheDocument();
  });
});
```

`site/src/page/__tests__/HomePage.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import HomePage from '../HomePage';
import { GET_SUMMARY } from 'hooks/queries';

const summary = {
  totalValueCents: 1234567,
  currency: 'USD',
  totalItems: 42,
  totalLocations: 7,
  totalTags: 5,
};

describe('HomePage', () => {
  it('renders the four statistics', async () => {
    render(
      <MockedProvider mocks={[{ request: { query: GET_SUMMARY }, result: { data: { summary } } }]}>
        <HomePage />
      </MockedProvider>,
    );
    expect(await screen.findByText('$12,345.67')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
    expect(screen.getByText('7')).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();
    for (const label of ['Total Value', 'Total Items', 'Total Locations', 'Total Tags']) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });
});
```

`site/src/App.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import App from './App';

// Warm the lazy chunks so a cold transform doesn't eat findBy's timeout when this
// file runs alone (see chore-tracker's App.test.tsx for the history).
beforeAll(async () => {
  await Promise.all([import('page/PageTemplate'), import('page/HomePage')]);
});

describe('App', () => {
  it('renders the shell and the home page', async () => {
    render(<App />);
    expect(await screen.findByRole('link', { name: 'Home Tracker' })).toBeInTheDocument();
    expect(await screen.findByText('Total Items')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run in `site/`: `yarn test --run`
Expected: the four new files fail on missing modules; theme tests still pass.

- [ ] **Step 3: Implement types, query, hooks, util**

`site/src/types/summary.ts`:

```ts
export interface Summary {
  totalValueCents: number;
  currency: string;
  totalItems: number;
  totalLocations: number;
  totalTags: number;
}
```

`site/src/hooks/queries.ts`:

```ts
import { gql } from '@apollo/client';

export const GET_SUMMARY = gql`
  query GetSummary {
    summary {
      totalValueCents
      currency
      totalItems
      totalLocations
      totalTags
    }
  }
`;
```

`site/src/hooks/useErrorToast.ts`:

```ts
import { useEffect } from 'react';
import { toast } from 'react-toastify';

/** Toasts `message` once whenever `error` becomes truthy. */
export function useErrorToast(error: unknown, message: string): void {
  useEffect(() => {
    if (error) toast.error(message);
  }, [error, message]);
}
```

`site/src/hooks/useSummary.ts`:

```ts
import { useQuery } from '@apollo/client/react';
import { Summary } from 'types/summary';
import { GET_SUMMARY } from './queries';
import { useErrorToast } from './useErrorToast';

interface SummaryResponse {
  summary: Summary;
}

export const useSummary = () => {
  const { data, loading, error } = useQuery<SummaryResponse>(GET_SUMMARY);
  useErrorToast(error, 'Error loading summary');
  return { summary: data?.summary ?? null, loading };
};
```

`site/src/utils/currency.ts`:

```ts
/** Formats integer minor units as money. Falls back to "1.00 CODE" for unknown codes. */
export function formatCents(cents: number, currency: string, locale?: string): string {
  const amount = cents / 100;
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(amount);
  } catch {
    return `${amount.toFixed(2)} ${currency}`;
  }
}
```

- [ ] **Step 4: Implement components and pages**

`site/src/components/StatCard.tsx`:

```tsx
interface StatCardProps {
  label: string;
  /** `null` renders a loading placeholder. */
  value: string | null;
}

export const StatCard = ({ label, value }: StatCardProps) => (
  <div className="rounded-xl border border-border bg-surface p-5 shadow-sm">
    <div className="text-sm font-medium text-muted">{label}</div>
    {value === null ? (
      <div
        aria-label={`${label} loading`}
        className="mt-2 h-8 w-24 animate-pulse rounded bg-surface-raised"
      />
    ) : (
      <div className="mt-2 text-3xl font-semibold text-text">{value}</div>
    )}
  </div>
);
```

`site/src/components/AppHeader.tsx`:

```tsx
import { Link } from 'react-router-dom';
import { useTheme } from 'theme/useTheme';

export const AppHeader = () => {
  const { theme, toggle } = useTheme();
  return (
    <header className="border-b border-border bg-surface">
      <div className="flex h-14 items-center justify-between px-4">
        <Link to="/" className="text-lg font-semibold text-text hover:text-accent">
          Home Tracker
        </Link>
        <button
          type="button"
          onClick={toggle}
          aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
          className="rounded-md border border-border px-3 py-1 text-sm text-muted hover:text-text"
        >
          {theme === 'dark' ? 'Light' : 'Dark'}
        </button>
      </div>
    </header>
  );
};
```

`site/src/components/Sidebar.tsx`:

```tsx
/** Location tree host. Phase 3 fills it; phase 1 only reserves the layout. */
export const Sidebar = () => (
  <aside className="hidden w-64 shrink-0 border-r border-border bg-surface p-4 md:block">
    <h2 className="text-sm font-semibold uppercase tracking-wide text-muted">Locations</h2>
    <p className="mt-2 text-sm text-muted">No locations yet.</p>
  </aside>
);
```

`site/src/page/PageTemplate.tsx`:

```tsx
import { Outlet } from 'react-router-dom';
import { AppHeader } from 'components/AppHeader';
import { Sidebar } from 'components/Sidebar';

export const PageTemplate = () => (
  <div className="flex min-h-screen flex-col bg-bg text-text">
    <AppHeader />
    <div className="flex flex-1">
      <Sidebar />
      <main className="flex-1 p-4 md:p-8">
        <Outlet />
      </main>
    </div>
  </div>
);

export default PageTemplate;
```

`site/src/page/HomePage.tsx`:

```tsx
import { StatCard } from 'components/StatCard';
import { useSummary } from 'hooks/useSummary';
import { formatCents } from 'utils/currency';

export const HomePage = () => {
  const { summary } = useSummary();
  return (
    <section>
      <h1>Home</h1>
      <div className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard
          label="Total Value"
          value={summary ? formatCents(summary.totalValueCents, summary.currency) : null}
        />
        <StatCard label="Total Items" value={summary ? String(summary.totalItems) : null} />
        <StatCard
          label="Total Locations"
          value={summary ? String(summary.totalLocations) : null}
        />
        <StatCard label="Total Tags" value={summary ? String(summary.totalTags) : null} />
      </div>
    </section>
  );
};

export default HomePage;
```

`site/src/App.tsx`:

```tsx
import React, { Suspense } from 'react';
import { ApolloClient, HttpLink, InMemoryCache } from '@apollo/client';
import { ApolloProvider } from '@apollo/client/react';
import { ToastContainer } from 'react-toastify';
import { RouterProvider, createBrowserRouter } from 'react-router-dom';
import 'react-toastify/dist/ReactToastify.css';
import { ThemeProvider } from 'theme/ThemeProvider';

const PageTemplate = React.lazy(() => import('page/PageTemplate'));
const HomePage = React.lazy(() => import('page/HomePage'));

const apolloClient = new ApolloClient({
  cache: new InMemoryCache(),
  link: new HttpLink({ uri: '/graphql', credentials: 'include' }),
});

const router = createBrowserRouter([
  {
    path: '/',
    element: <PageTemplate />,
    children: [{ index: true, element: <HomePage /> }],
  },
]);

const App = () => (
  <ThemeProvider>
    <ApolloProvider client={apolloClient}>
      <Suspense fallback={<div className="p-8 text-muted">Loading…</div>}>
        <ToastContainer theme="dark" />
        <RouterProvider router={router} />
      </Suspense>
    </ApolloProvider>
  </ThemeProvider>
);

export default App;
```

- [ ] **Step 5: Run tests, lint, build**

Run in `site/`: `yarn test --run && yarn lint && yarn build`
Expected: all test files pass, lint clean, build succeeds. If `react-toastify/dist/ReactToastify.css` does not resolve in v11, use `import 'react-toastify/ReactToastify.css';` instead.

- [ ] **Step 6: End-to-end check against the Rust server**

From the repo root: `cargo build --release && ./target/release/home-tracker` (the release build embeds the fresh `site/build`). Open `http://localhost:7008/` in a browser or run `curl -s localhost:7008/ | grep -o '<title>[^<]*'` → `<title>Home Tracker`. In the browser the four cards must show `$12,345.67`, `42`, `7`, `5`, and the theme button flips the page to the light stub and back, surviving a reload. Stop the server; `rm -rf data`.

- [ ] **Step 7: Commit**

```bash
git add site/src
git commit -m "feat(site): summary hook, stat cards, home page and app shell

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 8: README and CLAUDE.md

**Files:**
- Create: `README.md`, `CLAUDE.md`

- [ ] **Step 1: Write README.md**

Sections, each two to six lines: what it is (Homebox replacement, single home per instance, no auth yet); run with Docker (`docker run -p 7008:7008 -v ht-data:/data <image>`, env vars table from spec §12); local development (`just site-placeholder`, `cargo run`, `cd site && yarn dev`, `just test`, `cd site && yarn test`); importing a Homebox backup (`home-tracker import backup.zip`, marked "phase 2, not yet available"); project layout (the tree from spec §4 trimmed to what exists); link to the spec and plans under `docs/superpowers/`.

- [ ] **Step 2: Write CLAUDE.md**

Model it on `/home/steve/src/chore-tracker/CLAUDE.md`: Project Overview (two sentences), Commands (backend, frontend, database as in chore-tracker with port 7008 and `just` recipes), Architecture (backend modules list from spec §4 as they exist after this phase; frontend `theme/`, `hooks/`, `components/`, `page/`, `types/`, `utils/`), Key Conventions: GraphQL naming (`snake_case` → `camelCase`, no `is` prefix except `isLocation`), hooks own all GraphQL, errors via `toast.error`, semantic theme tokens only (list the ten classes and the rule against raw palette classes), `yarn` never `npm`, button cursor rule, Actor/require_write gate for mutations, `site/build` must exist before `cargo build` (`just site-placeholder`), the `homebox/` and `homebox-backup/` directories are reference material excluded from git. Keep it under 120 lines.

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: README and CLAUDE.md for the phase 1 skeleton

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

## Self-review

- **Spec coverage (Phase 1 exit criteria):** crate/clap/tracing/config/bind (T1), pool/PRAGMAs/migrations/settings (T2), `summary` + router + embed + serve (T3), healthcheck + import stub (T4), Docker/env/justfile/CI (T5), Vite scaffold + tokens + ThemeProvider (T6), PageTemplate/HomePage/StatCard/useSummary/vitest/eslint/husky (T6, T7), docs (T8). jemalloc on musl is in T1's `main.rs`. `DATA_DIR/originals` is created in T3's `serve`.
- **Placeholders:** none; every code step carries its code. The only "not implemented" text is the deliberate `import` stub the spec asks for.
- **Type consistency:** `build_pool`, `run_migrations`, `TestDb::new`, `svc::settings::currency`, `svc::stats::{Summary, summary}`, `GraphQLContext::new(pool, actor)`, `routes::app(pool)`, `healthcheck::{probe, probe_local, run}` are used with the same names and signatures in T2, T3, T4 and the tests. Frontend: `useTheme`/`ThemeProvider`/`THEME_STORAGE_KEY` (T6) are what T7 imports; `GET_SUMMARY`, `useSummary`, `formatCents`, `StatCard` are shared between the T7 tests and implementations.
- **Review Focus:** 1 → T1 `rejects_an_unparseable_port`; 2 → T2 `creates_the_parent_directory_of_the_database_file`; 3 → T2/T3 missing-currency tests; 4 → T3 `deep_links_fall_back_to_the_spa_shell`; 5 → T6 garbage/throwing storage tests.
