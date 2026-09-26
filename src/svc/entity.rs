//! Entities: locations and items, their hierarchy, and search.

use std::collections::HashSet;

use anyhow::{Context, Result};
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_types::Text;

use crate::models::Entity;
use crate::schema::{entities, entity_types};

/// Upper bound on parent hops, so damaged data can never loop forever.
const MAX_DEPTH: usize = 64;

/// The entity with `id`, if any.
pub fn get(conn: &mut SqliteConnection, id: &str) -> Result<Option<Entity>> {
    entities::table
        .find(id)
        .select(Entity::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading entity {id:?}"))
}

/// Whether `entity`'s type is a location type.
pub fn is_location(conn: &mut SqliteConnection, entity: &Entity) -> Result<bool> {
    entity_types::table
        .find(&entity.entity_type_id)
        .select(entity_types::is_location)
        .first(conn)
        .with_context(|| format!("loading the type of entity {:?}", entity.id))
}

/// Every direct child of `parent_id`, ordered by name (case-insensitive).
pub fn children(conn: &mut SqliteConnection, parent_id: &str) -> Result<Vec<Entity>> {
    entities::table
        .filter(entities::parent_id.eq(parent_id))
        .order(sql::<Text>("lower(name)"))
        .select(Entity::as_select())
        .load(conn)
        .with_context(|| format!("loading children of {parent_id:?}"))
}

/// The direct children of `parent_id` whose type is a location type.
pub fn child_locations(conn: &mut SqliteConnection, parent_id: &str) -> Result<Vec<Entity>> {
    children_by_kind(conn, parent_id, true)
}

/// The direct children of `parent_id` whose type is not a location type.
pub fn items(conn: &mut SqliteConnection, parent_id: &str) -> Result<Vec<Entity>> {
    children_by_kind(conn, parent_id, false)
}

fn children_by_kind(
    conn: &mut SqliteConnection,
    parent_id: &str,
    locations: bool,
) -> Result<Vec<Entity>> {
    entities::table
        .inner_join(entity_types::table)
        .filter(entities::parent_id.eq(parent_id))
        .filter(entity_types::is_location.eq(locations))
        .order(sql::<Text>("lower(entities.name)"))
        .select(Entity::as_select())
        .load(conn)
        .with_context(|| format!("loading children of {parent_id:?}"))
}

/// Items that are not inside any location.
pub fn root_items(conn: &mut SqliteConnection) -> Result<Vec<Entity>> {
    entities::table
        .inner_join(entity_types::table)
        .filter(entities::parent_id.is_null())
        .filter(entity_types::is_location.eq(false))
        .order(sql::<Text>("lower(entities.name)"))
        .select(Entity::as_select())
        .load(conn)
        .context("loading root items")
}

/// The chain of parents of `id`, root first, excluding `id` itself. Stops at a
/// missing parent, a repeated id (a cycle) or after [`MAX_DEPTH`] hops.
pub fn ancestors(conn: &mut SqliteConnection, id: &str) -> Result<Vec<Entity>> {
    let mut visited = HashSet::from([id.to_owned()]);
    let mut chain = Vec::new();
    let mut next = get(conn, id)?.and_then(|e| e.parent_id);
    while let Some(parent_id) = next {
        if chain.len() >= MAX_DEPTH || !visited.insert(parent_id.clone()) {
            break;
        }
        let Some(parent) = get(conn, &parent_id)? else {
            break;
        };
        next = parent.parent_id.clone();
        chain.push(parent);
    }
    chain.reverse();
    Ok(chain)
}

/// Every location, flat and sorted by name. Archived locations are included;
/// the client nests them into a tree itself using `parentId`.
pub fn locations(conn: &mut SqliteConnection) -> Result<Vec<Entity>> {
    entities::table
        .inner_join(entity_types::table)
        .filter(entity_types::is_location.eq(true))
        .order(sql::<Text>("lower(entities.name)"))
        .select(Entity::as_select())
        .load(conn)
        .context("loading locations")
}

/// Entities whose name contains `query` (case-insensitive, `%`/`_` literal),
/// non-archived first, then by name; at most `limit` rows (at least one, so a
/// non-positive limit never reaches SQLite, where `LIMIT -1` means unlimited).
pub fn search(conn: &mut SqliteConnection, query: &str, limit: i64) -> Result<Vec<Entity>> {
    entities::table
        .filter(
            entities::name
                .like(format!("%{}%", escape_like(query)))
                .escape('\\'),
        )
        .order((entities::archived.asc(), sql::<Text>("lower(name)")))
        .limit(limit.max(1))
        .select(Entity::as_select())
        .load(conn)
        .with_context(|| format!("searching entities for {query:?}"))
}

/// Makes `%`, `_` and the escape character itself match literally under `ESCAPE '\'`.
fn escape_like(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len());
    for c in query.chars() {
        if matches!(c, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ITEM_TYPE_ID, TestDb};
    use crate::schema::entities;
    use crate::svc::fixtures::{entity, seed_sample};

    fn names(rows: &[Entity]) -> Vec<&str> {
        rows.iter().map(|e| e.name.as_str()).collect()
    }

    fn insert(conn: &mut SqliteConnection, rows: &[Entity]) {
        diesel::insert_into(entities::table)
            .values(rows)
            .execute(conn)
            .unwrap();
    }

    #[test]
    fn children_are_sorted_case_insensitively() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let house = Some(ids.house.as_str());
        insert(
            &mut conn,
            &[
                entity("e-apple", "apple", ITEM_TYPE_ID, house, 20),
                entity("e-banana", "Banana", ITEM_TYPE_ID, house, 21),
            ],
        );
        let rows = children(&mut conn, &ids.house).unwrap();
        assert_eq!(
            names(&rows),
            ["apple", "Banana", "Broken lamp", "Garage", "Old TV"]
        );
    }

    #[test]
    fn child_locations_and_items_split_by_type() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let locations = child_locations(&mut conn, &ids.garage).unwrap();
        assert_eq!(names(&locations), ["Tote A"]);
        let things = items(&mut conn, &ids.garage).unwrap();
        assert_eq!(names(&things), ["Drill"]);
        assert!(is_location(&mut conn, &locations[0]).unwrap());
        assert!(!is_location(&mut conn, &things[0]).unwrap());
    }

    #[test]
    fn root_items_are_parentless_non_locations() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        assert_eq!(names(&root_items(&mut conn).unwrap()), ["Loose item"]);
    }

    #[test]
    fn ancestors_are_root_first() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        assert_eq!(
            names(&ancestors(&mut conn, &ids.tote_a).unwrap()),
            ["House", "Garage"]
        );
        assert!(ancestors(&mut conn, &ids.house).unwrap().is_empty());
        assert!(get(&mut conn, "missing").unwrap().is_none());
    }

    #[test]
    fn ancestors_terminate_on_a_cycle() {
        // Review Focus 3: bad imported data must not hang the request.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        diesel::sql_query("UPDATE entities SET parent_id = 'e-tote-a' WHERE id = 'e-house'")
            .execute(&mut conn)
            .unwrap();
        let chain = ancestors(&mut conn, &ids.tote_a).unwrap();
        let mut seen: Vec<&str> = chain.iter().map(|e| e.id.as_str()).collect();
        let total = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total, "an id repeated in {chain:?}");
        assert!(total <= 3);
    }

    #[test]
    fn locations_lists_every_location_type_entity_sorted() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        // Archived (Attic) and non-`Location`-typed (Tote A) locations are
        // still locations; the flat list has no notion of a tree to nest into.
        assert_eq!(
            names(&locations(&mut conn).unwrap()),
            ["Attic", "Garage", "House", "Tote A"]
        );
    }

    #[test]
    fn search_matches_name_case_insensitively_and_escapes_wildcards() {
        // Review Focus 5: user input must not act as a LIKE pattern.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        assert_eq!(names(&search(&mut conn, "dRiLl", 10).unwrap()), ["Drill"]);
        assert!(search(&mut conn, "%", 10).unwrap().is_empty());
        assert!(search(&mut conn, "_", 10).unwrap().is_empty());
        insert(
            &mut conn,
            &[
                entity("e-cotton", "100% cotton", ITEM_TYPE_ID, None, 20),
                entity("e-hundred", "1000 pieces", ITEM_TYPE_ID, None, 21),
            ],
        );
        assert_eq!(
            names(&search(&mut conn, "100%", 10).unwrap()),
            ["100% cotton"]
        );
    }

    #[test]
    fn search_lists_non_archived_first() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        insert(
            &mut conn,
            &[
                Entity {
                    archived: true,
                    ..entity("e-zeta-old", "Zeta", ITEM_TYPE_ID, None, 20)
                },
                entity("e-zeta", "Zeta", ITEM_TYPE_ID, None, 21),
            ],
        );
        let ids: Vec<String> = search(&mut conn, "zeta", 10)
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, ["e-zeta", "e-zeta-old"]);
        assert_eq!(search(&mut conn, "zeta", 1).unwrap().len(), 1);
        // A non-positive limit must not become SQLite's `LIMIT -1` (unlimited).
        assert_eq!(search(&mut conn, "Zeta", 0).unwrap().len(), 1);
    }
}
