//! Custom fields attached to entities.

use anyhow::{Context, Result};
use diesel::prelude::*;

use crate::models::EntityField;
use crate::schema::entity_fields;

/// The custom fields on `entity_id`, in creation order.
pub fn for_entity(conn: &mut SqliteConnection, entity_id: &str) -> Result<Vec<EntityField>> {
    entity_fields::table
        .filter(entity_fields::entity_id.eq(entity_id))
        .order((entity_fields::created_at.asc(), entity_fields::id.asc()))
        .select(EntityField::as_select())
        .load(conn)
        .with_context(|| format!("loading custom fields of entity {entity_id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::kinds::FieldKind;
    use crate::svc::fixtures::seed_sample;

    #[test]
    fn for_entity_returns_custom_fields() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let fields = for_entity(&mut conn, &ids.drill).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "Voltage");
        assert_eq!(fields[0].kind, FieldKind::Number);
        assert_eq!(fields[0].number_value, Some(18));
        assert!(for_entity(&mut conn, &ids.screws).unwrap().is_empty());
    }
}
