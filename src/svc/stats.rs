//! Home-page statistics. Phase 1 returned fixed numbers; phase 2 computes them.

use anyhow::{Context as _, Result};
use diesel::prelude::*;
use juniper::GraphQLObject;

use crate::money::Cents;
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

#[derive(QueryableByName)]
struct ValueRow {
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
    total: Option<f64>,
}

/// Homebox's formulas (spec §3 rows 7 and 8): value counts sold items and
/// multiplies by quantity; archived items are excluded from value and item
/// count; locations are counted whether archived or not.
pub fn summary(conn: &mut SqliteConnection) -> Result<Summary> {
    use crate::schema::{entities, entity_types, tags};

    let currency = settings::currency(conn)?;

    let non_location_active = entities::table
        .inner_join(entity_types::table)
        .filter(entity_types::is_location.eq(false))
        .filter(entities::archived.eq(false));

    let total_items: i64 = non_location_active.count().get_result(conn)?;
    let total_locations: i64 = entities::table
        .inner_join(entity_types::table)
        .filter(entity_types::is_location.eq(true))
        .count()
        .get_result(conn)?;
    let total_tags: i64 = tags::table.count().get_result(conn)?;

    let value: ValueRow = diesel::sql_query(
        "SELECT SUM(e.purchase_price_cents * e.quantity) AS total \
         FROM entities e JOIN entity_types t ON t.id = e.entity_type_id \
         WHERE t.is_location = 0 AND e.archived = 0",
    )
    .get_result(conn)
    .context("computing total value")?;
    let total_value_cents = Cents(value.total.unwrap_or(0.0).round() as i64);

    Ok(Summary {
        total_value_cents: total_value_cents.as_graphql_int(),
        currency,
        total_items: i32::try_from(total_items).unwrap_or(i32::MAX),
        total_locations: i32::try_from(total_locations).unwrap_or(i32::MAX),
        total_tags: i32::try_from(total_tags).unwrap_or(i32::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::schema::settings as settings_table;
    use crate::svc::fixtures::seed_sample;

    #[test]
    fn summary_matches_the_homebox_formulas_on_the_sample() {
        // Value = Drill 15299×1 + Screws 999×2 + Old TV 50000×1 (sold, still counted).
        // Broken lamp is archived (excluded), Loose item is 0, locations excluded.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        let s = summary(&mut conn).unwrap();
        assert_eq!(
            s,
            Summary {
                total_value_cents: 67_297,
                currency: "USD".to_owned(),
                total_items: 4,
                total_locations: 4,
                total_tags: 2,
            }
        );
    }

    #[test]
    fn an_empty_database_reports_zeros() {
        // Review focus 4: SUM over no rows is NULL in SQLite.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let s = summary(&mut conn).unwrap();
        assert_eq!(
            (
                s.total_value_cents,
                s.total_items,
                s.total_locations,
                s.total_tags
            ),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn fractional_quantities_round_to_cents() {
        use crate::db::ITEM_TYPE_ID;
        use crate::schema::entities;
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::insert_into(entities::table)
            .values((
                entities::id.eq("e-half"),
                entities::name.eq("Half"),
                entities::entity_type_id.eq(ITEM_TYPE_ID),
                entities::quantity.eq(1.5),
                entities::purchase_price_cents.eq(crate::money::Cents(333)),
            ))
            .execute(&mut conn)
            .unwrap();
        assert_eq!(summary(&mut conn).unwrap().total_value_cents, 500); // 499.5 rounds half away from zero
    }

    #[test]
    fn summary_reads_the_currency_from_settings() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::update(settings_table::table.filter(settings_table::key.eq("currency")))
            .set(settings_table::value.eq("EUR"))
            .execute(&mut conn)
            .unwrap();
        assert_eq!(summary(&mut conn).unwrap().currency, "EUR");
    }

    #[test]
    fn summary_fails_when_the_currency_row_is_gone() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::delete(settings_table::table)
            .execute(&mut conn)
            .unwrap();
        assert!(summary(&mut conn).is_err());
    }
}
