//! Entities: locations and items, their hierarchy, and search.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow, ensure};
use chrono::{NaiveDate, NaiveDateTime, Utc};
use diesel::dsl::{max, sql};
use diesel::prelude::*;
use diesel::sql_types::Text;
use uuid::Uuid;

use crate::asset_id::AssetId;
use crate::models::{Entity, TagEntity};
use crate::money::Cents;
use crate::schema::{entities, entity_types, tag_entities, tags};
use crate::svc::{attachment, entity_count_phrase, entity_type, optional_text, required_name};

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

/// What a caller supplies to create or update an entity. An update replaces
/// every editable field, so both take the full shape. Optional text is
/// trimmed and blank becomes `None`; `tag_ids: None` leaves the tags alone.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityInput {
    pub name: String,
    pub description: Option<String>,
    pub entity_type_id: String,
    pub parent_id: Option<String>,
    pub archived: bool,
    pub quantity: f64,
    pub insured: bool,
    pub serial_number: Option<String>,
    pub model_number: Option<String>,
    pub manufacturer: Option<String>,
    pub notes: Option<String>,
    pub lifetime_warranty: bool,
    pub warranty_expires: Option<NaiveDate>,
    pub warranty_details: Option<String>,
    pub purchase_date: Option<NaiveDate>,
    pub purchase_from: Option<String>,
    pub purchase_price_cents: Cents,
    pub sold_date: Option<NaiveDate>,
    pub sold_to: Option<String>,
    pub sold_price_cents: Cents,
    pub sold_notes: Option<String>,
    pub tag_ids: Option<Vec<String>>,
}

/// The editable columns of an [`EntityInput`] once validated and normalised;
/// written as-is by an update and, with id/asset id/`created_at`, by a create.
#[derive(Insertable, AsChangeset)]
#[diesel(table_name = entities, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
struct EntityChanges {
    name: String,
    description: Option<String>,
    entity_type_id: String,
    parent_id: Option<String>,
    archived: bool,
    quantity: f64,
    insured: bool,
    serial_number: Option<String>,
    model_number: Option<String>,
    manufacturer: Option<String>,
    notes: Option<String>,
    lifetime_warranty: bool,
    warranty_expires: Option<NaiveDate>,
    warranty_details: Option<String>,
    purchase_date: Option<NaiveDate>,
    purchase_from: Option<String>,
    purchase_price_cents: Cents,
    sold_date: Option<NaiveDate>,
    sold_to: Option<String>,
    sold_price_cents: Cents,
    sold_notes: Option<String>,
    updated_at: NaiveDateTime,
}

/// The entity with `id`, or a user-readable "not found" error.
pub(crate) fn require(conn: &mut SqliteConnection, id: &str) -> Result<Entity> {
    get(conn, id)?.ok_or_else(|| anyhow!("entity not found"))
}

/// The asset id the next created entity gets: one past the highest in use.
pub fn next_asset_id(conn: &mut SqliteConnection) -> Result<AssetId> {
    let highest: Option<AssetId> = entities::table
        .select(max(entities::asset_id))
        .first(conn)
        .context("finding the highest asset id")?;
    // Zero and negatives mean "no asset id", so numbering starts at 1.
    Ok(AssetId(highest.map_or(0, |id| id.0).max(0) + 1))
}

/// Refuses `parent_id` as the parent of an entity that is (`child_is_location`)
/// or is not a location. The parent must exist, a location may not go under an
/// item, and an existing entity `child_id` may not go under itself or any of
/// its descendants.
pub fn validate_parent(
    conn: &mut SqliteConnection,
    child_id: Option<&str>,
    child_is_location: bool,
    parent_id: &str,
) -> Result<()> {
    let parent = get(conn, parent_id)?.ok_or_else(|| anyhow!("parent not found"))?;
    if let Some(child_id) = child_id {
        ensure!(
            parent_id != child_id,
            "an entity cannot be placed inside itself"
        );
        ensure!(
            !ancestors(conn, parent_id)?.iter().any(|e| e.id == child_id),
            "cannot move an entity under its own descendant"
        );
    }
    ensure!(
        !child_is_location || is_location(conn, &parent)?,
        "a location cannot be placed under an item"
    );
    Ok(())
}

/// Validates `input` for a new entity (`id: None`) or existing entity `id`,
/// returning the columns to write and the tag ids to set, if any.
fn validated(
    conn: &mut SqliteConnection,
    id: Option<&str>,
    input: EntityInput,
) -> Result<(EntityChanges, Option<Vec<String>>)> {
    let name = required_name(&input.name)?;
    // Written so NaN fails too.
    ensure!(input.quantity >= 0.0, "quantity must not be negative");
    ensure!(
        input.purchase_price_cents >= Cents(0),
        "purchase price must not be negative"
    );
    ensure!(
        input.sold_price_cents >= Cents(0),
        "sold price must not be negative"
    );
    let entity_type = entity_type::get(conn, &input.entity_type_id)?
        .ok_or_else(|| anyhow!("entity type not found"))?;
    let parent_id = optional_text(input.parent_id);
    if let Some(parent_id) = &parent_id {
        validate_parent(conn, id, entity_type.is_location, parent_id)?;
    }
    // Retyping a location that holds locations as an item would leave those
    // locations under an item.
    if let Some(id) = id
        && !entity_type.is_location
    {
        ensure!(
            child_locations(conn, id)?.is_empty(),
            "{name} contains locations, so it must keep a location type"
        );
    }
    let changes = EntityChanges {
        name,
        description: optional_text(input.description),
        entity_type_id: entity_type.id,
        parent_id,
        archived: input.archived,
        quantity: input.quantity,
        insured: input.insured,
        serial_number: optional_text(input.serial_number),
        model_number: optional_text(input.model_number),
        manufacturer: optional_text(input.manufacturer),
        notes: optional_text(input.notes),
        lifetime_warranty: input.lifetime_warranty,
        warranty_expires: input.warranty_expires,
        warranty_details: optional_text(input.warranty_details),
        purchase_date: input.purchase_date,
        purchase_from: optional_text(input.purchase_from),
        purchase_price_cents: input.purchase_price_cents,
        sold_date: input.sold_date,
        sold_to: optional_text(input.sold_to),
        sold_price_cents: input.sold_price_cents,
        sold_notes: optional_text(input.sold_notes),
        updated_at: Utc::now().naive_utc(),
    };
    Ok((changes, input.tag_ids))
}

/// Creates an entity with the next asset id and, when given, its tags.
pub fn create(conn: &mut SqliteConnection, input: EntityInput) -> Result<Entity> {
    conn.transaction(|conn| {
        let (changes, tag_ids) = validated(conn, None, input)?;
        let id = Uuid::now_v7().to_string();
        let asset_id = next_asset_id(conn)?;
        diesel::insert_into(entities::table)
            .values((
                &changes,
                entities::id.eq(&id),
                entities::asset_id.eq(asset_id),
                entities::created_at.eq(changes.updated_at),
            ))
            .execute(conn)
            .with_context(|| format!("creating entity {:?}", changes.name))?;
        if let Some(tag_ids) = tag_ids {
            set_tags(conn, &id, &tag_ids)?;
        }
        require(conn, &id)
    })
}

/// Replaces the editable fields of entity `id` and, when given, its tags.
pub fn update(conn: &mut SqliteConnection, id: &str, input: EntityInput) -> Result<Entity> {
    conn.transaction(|conn| {
        require(conn, id)?;
        let (changes, tag_ids) = validated(conn, Some(id), input)?;
        diesel::update(entities::table.find(id))
            .set(&changes)
            .execute(conn)
            .with_context(|| format!("updating entity {id:?}"))?;
        if let Some(tag_ids) = tag_ids {
            set_tags(conn, id, &tag_ids)?;
        }
        require(conn, id)
    })
}

/// Deletes entity `id`; its tag links, custom fields, attachments and their
/// thumbnails go with it. Refused while it contains other entities. Originals
/// no remaining attachment shares are removed from `data_dir` only after the
/// delete commits, so a rollback never strands a row without its file.
pub fn delete(conn: &mut SqliteConnection, data_dir: &Path, id: &str) -> Result<()> {
    let orphans = conn.transaction(|conn| {
        let current = require(conn, id)?;
        let contained: i64 = entities::table
            .filter(entities::parent_id.eq(id))
            .count()
            .get_result(conn)
            .with_context(|| format!("counting children of {id:?}"))?;
        ensure!(
            contained == 0,
            "{} still contains {}; move them first",
            current.name,
            entity_count_phrase(contained)
        );
        let orphans = attachment::for_entity(conn, id)?
            .iter()
            .filter_map(|a| attachment::delete_row(conn, a).transpose())
            .collect::<Result<Vec<String>>>()?;
        diesel::delete(entities::table.find(id))
            .execute(conn)
            .with_context(|| format!("deleting entity {id:?}"))?;
        Ok(orphans)
    })?;
    for sha256 in &orphans {
        attachment::remove_original(conn, data_dir, sha256);
    }
    Ok(())
}

/// Replaces the tags of entity `id` with exactly `tag_ids` (duplicates
/// collapse); every tag must exist.
pub fn set_tags(conn: &mut SqliteConnection, id: &str, tag_ids: &[String]) -> Result<()> {
    let wanted: BTreeSet<&str> = tag_ids.iter().map(String::as_str).collect();
    conn.transaction(|conn| {
        require(conn, id)?;
        let found: Vec<String> = tags::table
            .filter(tags::id.eq_any(wanted.iter().copied()))
            .select(tags::id)
            .load(conn)
            .context("looking up tags")?;
        ensure!(found.len() == wanted.len(), "tag not found");
        diesel::delete(tag_entities::table.filter(tag_entities::entity_id.eq(id)))
            .execute(conn)
            .with_context(|| format!("clearing tags of entity {id:?}"))?;
        let links: Vec<TagEntity> = found
            .into_iter()
            .map(|tag_id| TagEntity {
                tag_id,
                entity_id: id.to_owned(),
            })
            .collect();
        diesel::insert_into(tag_entities::table)
            .values(&links)
            .execute(conn)
            .with_context(|| format!("tagging entity {id:?}"))?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Debug;
    use std::fs;

    use crate::db::{ITEM_TYPE_ID, LOCATION_TYPE_ID, TestDb};
    use crate::kinds::AttachmentKind;
    use crate::schema::{attachments, entities, entity_fields, tag_entities, thumbnails};
    use crate::svc::attachment::original_path;
    use crate::svc::fixtures::{attachment, entity, seed_sample};
    use crate::svc::tag;

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

    fn input(name: &str, type_id: &str, parent: Option<&str>) -> EntityInput {
        EntityInput {
            name: name.to_owned(),
            description: None,
            entity_type_id: type_id.to_owned(),
            parent_id: parent.map(str::to_owned),
            archived: false,
            quantity: 1.0,
            insured: false,
            serial_number: None,
            model_number: None,
            manufacturer: None,
            notes: None,
            lifetime_warranty: false,
            warranty_expires: None,
            warranty_details: None,
            purchase_date: None,
            purchase_from: None,
            purchase_price_cents: Cents(0),
            sold_date: None,
            sold_to: None,
            sold_price_cents: Cents(0),
            sold_notes: None,
            tag_ids: None,
        }
    }

    /// `input` carrying `current`'s editable fields, for update tests.
    fn input_from(current: &Entity) -> EntityInput {
        EntityInput {
            description: current.description.clone(),
            archived: current.archived,
            quantity: current.quantity,
            purchase_price_cents: current.purchase_price_cents,
            sold_price_cents: current.sold_price_cents,
            sold_date: current.sold_date,
            ..input(
                &current.name,
                &current.entity_type_id,
                current.parent_id.as_deref(),
            )
        }
    }

    fn entity_count(conn: &mut SqliteConnection) -> i64 {
        entities::table.count().get_result(conn).unwrap()
    }

    fn tag_ids(conn: &mut SqliteConnection, id: &str) -> Vec<String> {
        tag::for_entity(conn, id)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect()
    }

    fn message(result: Result<impl Debug>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn next_asset_id_starts_at_one() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        assert_eq!(next_asset_id(&mut conn).unwrap(), AssetId(1));
    }

    #[test]
    fn create_assigns_the_next_asset_id() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let first = create(&mut conn, input("Hammer", ITEM_TYPE_ID, Some(&ids.garage))).unwrap();
        let second = create(&mut conn, input("Saw", ITEM_TYPE_ID, None)).unwrap();
        assert_eq!(first.asset_id, AssetId(6));
        assert_eq!(second.asset_id, AssetId(7));
        assert_eq!(first.parent_id.as_deref(), Some(ids.garage.as_str()));
        assert_eq!(first.created_at, first.updated_at);
        assert_eq!(get(&mut conn, &first.id).unwrap(), Some(first));
    }

    #[test]
    fn create_trims_and_rejects_blank_names() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        let created = create(
            &mut conn,
            EntityInput {
                description: Some("   ".to_owned()),
                notes: Some("  oiled ".to_owned()),
                parent_id: Some(" ".to_owned()),
                ..input("  Hammer \t", ITEM_TYPE_ID, None)
            },
        )
        .unwrap();
        assert_eq!(created.name, "Hammer");
        assert_eq!(created.description, None);
        assert_eq!(created.notes.as_deref(), Some("oiled"));
        assert_eq!(created.parent_id, None);

        let before = entity_count(&mut conn);
        let err = message(create(&mut conn, input(" \t ", ITEM_TYPE_ID, None)));
        assert_eq!(err, "name must not be blank");
        assert_eq!(entity_count(&mut conn), before);
    }

    #[test]
    fn create_rejects_unknown_type_and_parent() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        let before = entity_count(&mut conn);
        let err = message(create(&mut conn, input("Hammer", "missing", None)));
        assert_eq!(err, "entity type not found");
        let err = message(create(
            &mut conn,
            input("Hammer", ITEM_TYPE_ID, Some("missing")),
        ));
        assert_eq!(err, "parent not found");
        assert_eq!(entity_count(&mut conn), before);
    }

    #[test]
    fn create_rejects_a_location_under_an_item() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let err = message(create(
            &mut conn,
            input("Shelf", LOCATION_TYPE_ID, Some(&ids.drill)),
        ));
        assert_eq!(err, "a location cannot be placed under an item");
    }

    #[test]
    fn create_allows_an_item_under_an_item() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let bit = create(
            &mut conn,
            input("Drill bit", ITEM_TYPE_ID, Some(&ids.drill)),
        )
        .unwrap();
        assert_eq!(bit.parent_id.as_deref(), Some(ids.drill.as_str()));

        let screws = get(&mut conn, &ids.screws).unwrap().unwrap();
        let moved = update(
            &mut conn,
            &ids.screws,
            EntityInput {
                parent_id: Some(ids.drill.clone()),
                ..input_from(&screws)
            },
        )
        .unwrap();
        assert_eq!(moved.parent_id.as_deref(), Some(ids.drill.as_str()));
    }

    #[test]
    fn create_sets_tags_and_rolls_back_on_an_unknown_tag() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let tagged = create(
            &mut conn,
            EntityInput {
                tag_ids: Some(vec![ids.tools.clone(), ids.electronics.clone()]),
                ..input("Soldering iron", ITEM_TYPE_ID, None)
            },
        )
        .unwrap();
        assert_eq!(
            tag_ids(&mut conn, &tagged.id),
            [ids.electronics.as_str(), ids.tools.as_str()]
        );

        let before = entity_count(&mut conn);
        let err = message(create(
            &mut conn,
            EntityInput {
                tag_ids: Some(vec!["missing".to_owned()]),
                ..input("Multimeter", ITEM_TYPE_ID, None)
            },
        ));
        assert_eq!(err, "tag not found");
        assert_eq!(entity_count(&mut conn), before);
    }

    #[test]
    fn update_moves_and_replaces_tags() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let before = get(&mut conn, &ids.drill).unwrap().unwrap();
        let updated = update(
            &mut conn,
            &ids.drill,
            EntityInput {
                parent_id: Some(ids.house.clone()),
                tag_ids: Some(vec![ids.electronics.clone()]),
                serial_number: Some(" SN-1 ".to_owned()),
                ..input_from(&before)
            },
        )
        .unwrap();
        assert_eq!(updated.parent_id.as_deref(), Some(ids.house.as_str()));
        assert_eq!(updated.serial_number.as_deref(), Some("SN-1"));
        assert_eq!(updated.asset_id, before.asset_id);
        assert_eq!(updated.created_at, before.created_at);
        assert!(updated.updated_at > before.updated_at);
        assert_eq!(tag_ids(&mut conn, &ids.drill), [ids.electronics.as_str()]);

        // Omitting `tag_ids` leaves the tags alone.
        update(&mut conn, &ids.drill, input_from(&updated)).unwrap();
        assert_eq!(tag_ids(&mut conn, &ids.drill), [ids.electronics.as_str()]);
    }

    #[test]
    fn update_refuses_a_cycle() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let house = get(&mut conn, &ids.house).unwrap().unwrap();
        let err = message(update(
            &mut conn,
            &ids.house,
            EntityInput {
                parent_id: Some(ids.tote_a.clone()),
                ..input_from(&house)
            },
        ));
        assert_eq!(err, "cannot move an entity under its own descendant");
        assert_eq!(get(&mut conn, &ids.house).unwrap(), Some(house));

        let tote = get(&mut conn, &ids.tote_a).unwrap().unwrap();
        let err = message(update(
            &mut conn,
            &ids.tote_a,
            EntityInput {
                parent_id: Some(ids.tote_a.clone()),
                ..input_from(&tote)
            },
        ));
        assert_eq!(err, "an entity cannot be placed inside itself");
    }

    #[test]
    fn update_rejects_negative_quantity_and_cents() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let screws = get(&mut conn, &ids.screws).unwrap().unwrap();
        let cases = [
            (
                EntityInput {
                    quantity: -1.0,
                    ..input_from(&screws)
                },
                "quantity must not be negative",
            ),
            (
                EntityInput {
                    quantity: f64::NAN,
                    ..input_from(&screws)
                },
                "quantity must not be negative",
            ),
            (
                EntityInput {
                    purchase_price_cents: Cents(-1),
                    ..input_from(&screws)
                },
                "purchase price must not be negative",
            ),
            (
                EntityInput {
                    sold_price_cents: Cents(-1),
                    ..input_from(&screws)
                },
                "sold price must not be negative",
            ),
        ];
        for (bad, expected) in cases {
            assert_eq!(message(update(&mut conn, &ids.screws, bad)), expected);
        }
        assert_eq!(get(&mut conn, &ids.screws).unwrap(), Some(screws));
    }

    #[test]
    fn update_keeps_a_location_holding_locations_a_location() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let garage = get(&mut conn, &ids.garage).unwrap().unwrap();
        let err = message(update(
            &mut conn,
            &ids.garage,
            EntityInput {
                entity_type_id: ITEM_TYPE_ID.to_owned(),
                ..input_from(&garage)
            },
        ));
        assert_eq!(
            err,
            "Garage contains locations, so it must keep a location type"
        );

        // Tote A holds only Screws, an item, so it may become one.
        let tote = get(&mut conn, &ids.tote_a).unwrap().unwrap();
        let retyped = update(
            &mut conn,
            &ids.tote_a,
            EntityInput {
                entity_type_id: ITEM_TYPE_ID.to_owned(),
                ..input_from(&tote)
            },
        )
        .unwrap();
        assert_eq!(retyped.entity_type_id, ITEM_TYPE_ID);
    }

    #[test]
    fn update_rejects_an_unknown_entity() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        let err = message(update(&mut conn, "missing", input("X", ITEM_TYPE_ID, None)));
        assert_eq!(err, "entity not found");
    }

    #[test]
    fn delete_refuses_with_children() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let data = tempfile::tempdir().unwrap();
        let err = message(delete(&mut conn, data.path(), &ids.garage));
        assert_eq!(err, "Garage still contains 2 entities; move them first");
        assert!(get(&mut conn, &ids.garage).unwrap().is_some());
        assert_eq!(
            message(delete(&mut conn, data.path(), "missing")),
            "entity not found"
        );
    }

    /// Writes the sample's two Drill originals under a fresh data dir.
    fn write_drill_originals() -> tempfile::TempDir {
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(data.path().join("originals")).unwrap();
        for sha in ["aa".repeat(32), "bb".repeat(32)] {
            fs::write(original_path(data.path(), &sha), b"bytes").unwrap();
        }
        data
    }

    #[test]
    fn delete_removes_an_item_and_its_tags_and_attachments() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let data = write_drill_originals();
        delete(&mut conn, data.path(), &ids.drill).unwrap();

        assert!(get(&mut conn, &ids.drill).unwrap().is_none());
        let links: i64 = tag_entities::table
            .filter(tag_entities::entity_id.eq(&ids.drill))
            .count()
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(links, 0);
        let attached: i64 = attachments::table.count().get_result(&mut conn).unwrap();
        assert_eq!(attached, 0);
        let thumbs: i64 = thumbnails::table.count().get_result(&mut conn).unwrap();
        assert_eq!(thumbs, 0);
        let fields: i64 = entity_fields::table.count().get_result(&mut conn).unwrap();
        assert_eq!(fields, 0);
        for sha in ["aa".repeat(32), "bb".repeat(32)] {
            assert!(
                !original_path(data.path(), &sha).exists(),
                "{sha} left behind"
            );
        }
    }

    #[test]
    fn delete_keeps_an_original_another_attachment_shares() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let data = write_drill_originals();
        let shared = "aa".repeat(32);
        diesel::insert_into(attachments::table)
            .values(attachment(
                "a-screws-photo",
                &ids.screws,
                AttachmentKind::Photo,
                true,
                &shared,
                3,
                20,
            ))
            .execute(&mut conn)
            .unwrap();
        delete(&mut conn, data.path(), &ids.drill).unwrap();

        assert!(original_path(data.path(), &shared).exists());
        assert!(!original_path(data.path(), &"bb".repeat(32)).exists());
    }

    #[test]
    fn set_tags_rejects_unknown_ids() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let err = message(set_tags(
            &mut conn,
            &ids.drill,
            &[ids.electronics.clone(), "missing".to_owned()],
        ));
        assert_eq!(err, "tag not found");
        assert_eq!(tag_ids(&mut conn, &ids.drill), [ids.tools.as_str()]);

        // Duplicates collapse, and an empty set clears.
        set_tags(
            &mut conn,
            &ids.drill,
            &[ids.electronics.clone(), ids.electronics.clone()],
        )
        .unwrap();
        assert_eq!(tag_ids(&mut conn, &ids.drill), [ids.electronics.as_str()]);
        set_tags(&mut conn, &ids.drill, &[]).unwrap();
        assert!(tag_ids(&mut conn, &ids.drill).is_empty());
        assert_eq!(
            message(set_tags(&mut conn, "missing", &[])),
            "entity not found"
        );
    }
}
