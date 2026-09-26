//! Entity types: the built-in `Location`/`Item` plus user-defined kinds.

use anyhow::{Context, Result, anyhow, ensure};
use chrono::Utc;
use diesel::alias;
use diesel::dsl::{count, sql};
use diesel::prelude::*;
use diesel::sql_types::Text;
use uuid::Uuid;

use crate::db::{ITEM_TYPE_ID, LOCATION_TYPE_ID};
use crate::models::EntityType;
use crate::schema::{entities, entity_types};
use crate::svc::{entity_count_phrase, optional_text, required_name};

/// The fields a caller supplies to create an entity type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntityType {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub is_location: bool,
}

/// An update replaces every editable field, so it carries the same shape as a create.
pub type EntityTypeChanges = NewEntityType;

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

/// The type with `id`, or a user-readable "not found" error.
fn require(conn: &mut SqliteConnection, id: &str) -> Result<EntityType> {
    get(conn, id)?.ok_or_else(|| anyhow!("entity type not found"))
}

/// How many entities of this type directly contain at least one location.
/// While this is non-zero the type cannot stop being a location type, or a
/// location would end up under an item.
pub fn location_child_count(conn: &mut SqliteConnection, type_id: &str) -> Result<i64> {
    let parents = alias!(entities as parents);
    entities::table
        .inner_join(entity_types::table)
        .inner_join(parents.on(entities::parent_id.eq(parents.field(entities::id).nullable())))
        .filter(entity_types::is_location.eq(true))
        .filter(parents.field(entities::entity_type_id).eq(type_id))
        .select(count(parents.field(entities::id)).aggregate_distinct())
        .get_result(conn)
        .with_context(|| format!("counting location parents of type {type_id:?}"))
}

/// Creates a type; the name is trimmed and must not be blank.
pub fn create(conn: &mut SqliteConnection, input: NewEntityType) -> Result<EntityType> {
    let now = Utc::now().naive_utc();
    let row = EntityType {
        id: Uuid::now_v7().to_string(),
        name: required_name(&input.name)?,
        description: optional_text(input.description),
        icon: optional_text(input.icon),
        is_location: input.is_location,
        default_template_id: None,
        created_at: now,
        updated_at: now,
    };
    diesel::insert_into(entity_types::table)
        .values(&row)
        .execute(conn)
        .with_context(|| format!("creating entity type {:?}", row.name))?;
    require(conn, &row.id)
}

/// Replaces the editable fields of type `id`. Turning a location type into an
/// item type is refused while any entity of the type contains a location.
pub fn update(
    conn: &mut SqliteConnection,
    id: &str,
    changes: EntityTypeChanges,
) -> Result<EntityType> {
    let name = required_name(&changes.name)?;
    conn.transaction(|conn| {
        let current = require(conn, id)?;
        if current.is_location && !changes.is_location {
            let holders = location_child_count(conn, id)?;
            ensure!(
                holders == 0,
                "cannot make {} a non-location type while {} of that type contain location \
                 children; move them first",
                current.name,
                entity_count_phrase(holders)
            );
        }
        diesel::update(entity_types::table.find(id))
            .set((
                entity_types::name.eq(name),
                entity_types::description.eq(optional_text(changes.description)),
                entity_types::icon.eq(optional_text(changes.icon)),
                entity_types::is_location.eq(changes.is_location),
                entity_types::updated_at.eq(Utc::now().naive_utc()),
            ))
            .execute(conn)
            .with_context(|| format!("updating entity type {id:?}"))?;
        require(conn, id)
    })
}

/// Deletes type `id`. The built-in `Location`/`Item` types and any type still
/// used by an entity are refused.
pub fn delete(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    conn.transaction(|conn| {
        let current = require(conn, id)?;
        ensure!(
            ![LOCATION_TYPE_ID, ITEM_TYPE_ID].contains(&id),
            "{} is a built-in type and cannot be deleted",
            current.name
        );
        let users = entity_count(conn, id)?;
        ensure!(
            users == 0,
            "{} is still used by {}; change their type first",
            current.name,
            entity_count_phrase(users)
        );
        diesel::delete(entity_types::table.find(id))
            .execute(conn)
            .with_context(|| format!("deleting entity type {id:?}"))?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::svc::fixtures::{self, seed_sample};

    fn input(name: &str, is_location: bool) -> NewEntityType {
        NewEntityType {
            name: name.to_owned(),
            description: Some("desc".to_owned()),
            icon: None,
            is_location,
        }
    }

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

    #[test]
    fn create_trims_the_name_and_rejects_blank() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let created = create(&mut conn, input("  Shelf  ", true)).unwrap();
        assert_eq!(created.name, "Shelf");
        assert_eq!(created.description.as_deref(), Some("desc"));
        assert!(created.is_location);
        assert_eq!(get(&mut conn, &created.id).unwrap(), Some(created));

        let err = create(&mut conn, input(" \t ", false)).unwrap_err();
        assert!(err.to_string().contains("name"), "{err}");
    }

    #[test]
    fn update_refuses_location_to_item_while_location_children_exist() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        diesel::insert_into(entities::table)
            .values(fixtures::entity(
                "e-pouch",
                "Pouch",
                LOCATION_TYPE_ID,
                Some(&ids.tote_a),
                50,
            ))
            .execute(&mut conn)
            .unwrap();
        assert_eq!(location_child_count(&mut conn, &ids.tote_type).unwrap(), 1);

        let err = update(&mut conn, &ids.tote_type, input("Tote", false)).unwrap_err();
        assert!(err.to_string().contains("location children"), "{err}");
        assert!(get(&mut conn, &ids.tote_type).unwrap().unwrap().is_location);
    }

    #[test]
    fn update_allows_location_to_item_when_no_location_children() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let before = get(&mut conn, &ids.tote_type).unwrap().unwrap();
        // Tote A holds only Screws, an item.
        assert_eq!(location_child_count(&mut conn, &ids.tote_type).unwrap(), 0);

        let updated = update(&mut conn, &ids.tote_type, input(" Bin ", false)).unwrap();
        assert_eq!(updated.name, "Bin");
        assert!(!updated.is_location);
        assert_eq!(updated.description.as_deref(), Some("desc"));
        assert_eq!(updated.created_at, before.created_at);
        assert!(updated.updated_at > before.updated_at);
    }

    #[test]
    fn update_rejects_blank_names_and_unknown_ids() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        assert!(update(&mut conn, &ids.tote_type, input("  ", true)).is_err());
        let err = update(&mut conn, "missing", input("X", true)).unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");
    }

    #[test]
    fn delete_refuses_a_type_in_use() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let err = delete(&mut conn, &ids.tote_type).unwrap_err();
        assert!(err.to_string().contains("1 entity"), "{err}");
        assert!(get(&mut conn, &ids.tote_type).unwrap().is_some());
    }

    #[test]
    fn delete_removes_an_unused_type() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        let shelf = create(&mut conn, input("Shelf", true)).unwrap();
        delete(&mut conn, &shelf.id).unwrap();
        assert!(get(&mut conn, &shelf.id).unwrap().is_none());
        assert!(delete(&mut conn, &shelf.id).is_err());
    }

    #[test]
    fn delete_refuses_the_seeded_built_ins() {
        // Even unused: a fresh database has no entities at all.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        for id in [LOCATION_TYPE_ID, ITEM_TYPE_ID] {
            let err = delete(&mut conn, id).unwrap_err();
            assert!(err.to_string().contains("built-in"), "{err}");
            assert!(get(&mut conn, id).unwrap().is_some());
        }
    }

    #[test]
    fn blank_optional_text_is_stored_as_none() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let blanks = NewEntityType {
            description: Some("  ".to_owned()),
            icon: Some(String::new()),
            ..input("Shelf", true)
        };
        let created = create(&mut conn, blanks.clone()).unwrap();
        assert_eq!((created.description, created.icon), (None, None));
        let updated = update(
            &mut conn,
            &created.id,
            NewEntityType {
                icon: Some(" box ".to_owned()),
                ..blanks
            },
        )
        .unwrap();
        assert_eq!(updated.description, None);
        assert_eq!(updated.icon.as_deref(), Some("box"));
    }
}
