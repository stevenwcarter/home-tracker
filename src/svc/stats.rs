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
        diesel::delete(settings_table::table)
            .execute(&mut conn)
            .unwrap();
        assert!(summary(&mut conn).is_err());
    }
}
