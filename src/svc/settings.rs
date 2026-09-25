//! Key/value settings, seeded by migrations.

use anyhow::{Context, Result};
use diesel::prelude::*;

use crate::schema::settings;

pub const CURRENCY_KEY: &str = "currency";

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
