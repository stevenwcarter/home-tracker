//! Entity types: the built-in `Location`/`Item` plus user-defined kinds.

use anyhow::{Context, Result};
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_types::Text;

use crate::models::EntityType;
use crate::schema::{entities, entity_types};

/// Every type, ordered by name (case-insensitive).
pub fn list(conn: &mut SqliteConnection) -> Result<Vec<EntityType>> {
    entity_types::table
        .order(sql::<Text>("lower(name)"))
        .select(EntityType::as_select())
        .load(conn)
        .context("listing entity types")
}

/// The type with `id`, if any.
pub fn get(conn: &mut SqliteConnection, id: &str) -> Result<Option<EntityType>> {
    entity_types::table
        .find(id)
        .select(EntityType::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading entity type {id:?}"))
}

/// How many entities (archived included) have this type.
pub fn entity_count(conn: &mut SqliteConnection, type_id: &str) -> Result<i64> {
    entities::table
        .filter(entities::entity_type_id.eq(type_id))
        .count()
        .get_result(conn)
        .with_context(|| format!("counting entities of type {type_id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{LOCATION_TYPE_ID, TestDb};
    use crate::svc::fixtures::seed_sample;

    #[test]
    fn list_is_ordered_by_name() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        let names: Vec<String> = list(&mut conn)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, ["Item", "Location", "Tote"]);
    }

    #[test]
    fn entity_count_counts_all_entities_of_the_type() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        assert_eq!(entity_count(&mut conn, LOCATION_TYPE_ID).unwrap(), 3);
        assert_eq!(entity_count(&mut conn, &ids.tote_type).unwrap(), 1);
        let tote = get(&mut conn, &ids.tote_type).unwrap().unwrap();
        assert!(tote.is_location);
        assert!(get(&mut conn, "missing").unwrap().is_none());
    }
}
