//! SQLite connection pool and embedded migrations.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, CustomizeConnection, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

/// Seeded built-in entity type ids (see the inventory migration). The importer
/// maps Homebox's `global.location` / `global.item` onto these by name.
pub const LOCATION_TYPE_ID: &str = "00000000-0000-7000-8000-000000000001";
pub const ITEM_TYPE_ID: &str = "00000000-0000-7000-8000-000000000002";

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

/// Sets the per-connection PRAGMAs. `busy_timeout` goes first so a locked
/// database waits instead of failing the PRAGMAs that follow.
#[derive(Debug)]
struct ConnectionOptions {
    busy_timeout: Duration,
}

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            diesel::sql_query(format!(
                "PRAGMA busy_timeout = {};",
                self.busy_timeout.as_millis()
            ))
            .execute(conn)?;
            diesel::sql_query("PRAGMA journal_mode = WAL;").execute(conn)?;
            diesel::sql_query("PRAGMA foreign_keys = ON;").execute(conn)?;
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}

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

/// A throwaway database for tests: a temp directory holding a fresh SQLite file
/// with all migrations applied. The directory is removed when this is dropped.
pub struct TestDb {
    pub pool: SqlitePool,
    _dir: tempfile::TempDir,
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

    #[test]
    fn wal_and_busy_timeout_are_set_on_every_connection() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let mode = diesel::sql_query("PRAGMA journal_mode")
            .get_result::<JournalMode>(&mut conn)
            .unwrap();
        assert_eq!(mode.journal_mode.to_lowercase(), "wal");
        let timeout = diesel::sql_query("PRAGMA busy_timeout")
            .get_result::<BusyTimeout>(&mut conn)
            .unwrap();
        assert_eq!(timeout.timeout, 5000);
    }

    #[test]
    fn built_in_types_are_seeded() {
        use crate::schema::entity_types::dsl::*;
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let rows: Vec<(String, String, bool)> = entity_types
            .select((id, name, is_location))
            .order(name.asc())
            .load(&mut conn)
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (ITEM_TYPE_ID.to_owned(), "Item".to_owned(), false),
                (LOCATION_TYPE_ID.to_owned(), "Location".to_owned(), true),
            ]
        );
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

    #[derive(QueryableByName)]
    struct JournalMode {
        #[diesel(sql_type = diesel::sql_types::Text)]
        journal_mode: String,
    }

    #[derive(QueryableByName)]
    struct BusyTimeout {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        timeout: i32,
    }
}
