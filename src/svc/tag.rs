//! Tags and their assignments to entities.

use anyhow::{Context, Result};
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_types::Text;

use crate::models::Tag;
use crate::schema::{tag_entities, tags};

/// Every tag, ordered by name (case-insensitive).
pub fn list(conn: &mut SqliteConnection) -> Result<Vec<Tag>> {
    tags::table
        .order(sql::<Text>("lower(name)"))
        .select(Tag::as_select())
        .load(conn)
        .context("listing tags")
}

/// The tag with `id`, if any.
pub fn get(conn: &mut SqliteConnection, id: &str) -> Result<Option<Tag>> {
    tags::table
        .find(id)
        .select(Tag::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading tag {id:?}"))
}

/// The tags assigned to `entity_id`, ordered by name (case-insensitive).
pub fn for_entity(conn: &mut SqliteConnection, entity_id: &str) -> Result<Vec<Tag>> {
    tags::table
        .inner_join(tag_entities::table)
        .filter(tag_entities::entity_id.eq(entity_id))
        .order(sql::<Text>("lower(tags.name)"))
        .select(Tag::as_select())
        .load(conn)
        .with_context(|| format!("loading tags of entity {entity_id:?}"))
}

/// How many entities carry this tag directly (child tags are not rolled up).
pub fn entity_count(conn: &mut SqliteConnection, tag_id: &str) -> Result<i64> {
    tag_entities::table
        .filter(tag_entities::tag_id.eq(tag_id))
        .count()
        .get_result(conn)
        .with_context(|| format!("counting entities tagged {tag_id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::svc::fixtures::seed_sample;

    #[test]
    fn for_entity_returns_assigned_tags() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let tags: Vec<String> = for_entity(&mut conn, &ids.drill)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(tags, [ids.tools.as_str()]);
        assert!(for_entity(&mut conn, &ids.screws).unwrap().is_empty());
        let names: Vec<String> = list(&mut conn)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, ["Electronics", "Tools"]);
        let electronics = get(&mut conn, &ids.electronics).unwrap().unwrap();
        assert_eq!(electronics.parent_id.as_deref(), Some(ids.tools.as_str()));
    }

    #[test]
    fn entity_count() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        assert_eq!(super::entity_count(&mut conn, &ids.tools).unwrap(), 1);
        assert_eq!(super::entity_count(&mut conn, &ids.electronics).unwrap(), 0);
    }
}
