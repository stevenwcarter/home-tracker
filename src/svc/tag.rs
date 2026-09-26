//! Tags and their assignments to entities.

use std::collections::HashSet;

use anyhow::{Context, Result, anyhow, ensure};
use chrono::Utc;
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::sql_types::Text;
use uuid::Uuid;

use crate::models::Tag;
use crate::schema::{tag_entities, tags};
use crate::svc::{optional_text, required_name};

/// The fields a caller supplies to create a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTag {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub parent_id: Option<String>,
}

/// An update replaces every editable field, so it carries the same shape as a create.
pub type TagChanges = NewTag;

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

/// The tag with `id`, or a user-readable "not found" error.
fn require(conn: &mut SqliteConnection, id: &str) -> Result<Tag> {
    get(conn, id)?.ok_or_else(|| anyhow!("tag not found"))
}

/// Refuses a `parent_id` that does not exist or, for an existing tag `id`,
/// that is the tag itself or one of its descendants.
fn validate_parent(conn: &mut SqliteConnection, id: Option<&str>, parent_id: &str) -> Result<()> {
    let parent = get(conn, parent_id)?.ok_or_else(|| anyhow!("parent tag not found"))?;
    let Some(id) = id else {
        return Ok(());
    };
    // The visited set stops the walk on a cycle already in the data instead
    // of looping forever.
    let mut visited = HashSet::new();
    let mut cursor = Some(parent);
    while let Some(tag) = cursor {
        ensure!(
            tag.id != id,
            "a tag cannot be nested under itself or its descendants (cycle)"
        );
        if !visited.insert(tag.id) {
            break;
        }
        cursor = tag
            .parent_id
            .map(|next| get(conn, &next))
            .transpose()?
            .flatten();
    }
    Ok(())
}

/// A trimmed colour, which must be `#rgb`, `#rrggbb` or `#rrggbbaa` (any
/// case); blank is no colour. The site paints it as an inline background, so
/// nothing else may reach it, including through the importer. Keep in step
/// with the site's guard, `isHexColor` in `site/src/utils/color.ts`.
pub(crate) fn optional_color(color: Option<String>) -> Result<Option<String>> {
    let Some(color) = optional_text(color) else {
        return Ok(None);
    };
    let valid = color.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
    });
    ensure!(valid, "colour must be a hex value like #a1b2c3");
    Ok(Some(color))
}

/// Creates a tag; the name is trimmed and must not be blank, the colour (when
/// given) must be a hex value, and the parent (when given) must exist.
pub fn create(conn: &mut SqliteConnection, input: NewTag) -> Result<Tag> {
    let name = required_name(&input.name)?;
    let color = optional_color(input.color)?;
    let parent_id = optional_text(input.parent_id);
    conn.transaction(|conn| {
        if let Some(parent_id) = &parent_id {
            validate_parent(conn, None, parent_id)?;
        }
        let now = Utc::now().naive_utc();
        let row = Tag {
            id: Uuid::now_v7().to_string(),
            name,
            description: optional_text(input.description),
            color,
            icon: optional_text(input.icon),
            parent_id,
            created_at: now,
            updated_at: now,
        };
        diesel::insert_into(tags::table)
            .values(&row)
            .execute(conn)
            .with_context(|| format!("creating tag {:?}", row.name))?;
        require(conn, &row.id)
    })
}

/// Replaces the editable fields of tag `id`. The colour (when given) must be
/// a hex value, and the new parent must exist and must not be the tag itself
/// or one of its descendants.
pub fn update(conn: &mut SqliteConnection, id: &str, changes: TagChanges) -> Result<Tag> {
    let name = required_name(&changes.name)?;
    let color = optional_color(changes.color)?;
    let parent_id = optional_text(changes.parent_id);
    conn.transaction(|conn| {
        require(conn, id)?;
        if let Some(parent_id) = &parent_id {
            validate_parent(conn, Some(id), parent_id)?;
        }
        diesel::update(tags::table.find(id))
            .set((
                tags::name.eq(name),
                tags::description.eq(optional_text(changes.description)),
                tags::color.eq(color),
                tags::icon.eq(optional_text(changes.icon)),
                tags::parent_id.eq(parent_id),
                tags::updated_at.eq(Utc::now().naive_utc()),
            ))
            .execute(conn)
            .with_context(|| format!("updating tag {id:?}"))?;
        require(conn, id)
    })
}

/// Deletes tag `id`. Its entity links go with it and its child tags become
/// top-level (both by foreign key).
pub fn delete(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    let deleted = diesel::delete(tags::table.find(id))
        .execute(conn)
        .with_context(|| format!("deleting tag {id:?}"))?;
    ensure!(deleted > 0, "tag not found");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::svc::fixtures::seed_sample;

    fn input(name: &str, parent_id: Option<&str>) -> NewTag {
        NewTag {
            name: name.to_owned(),
            description: None,
            color: Some("#ff0000".to_owned()),
            icon: None,
            parent_id: parent_id.map(str::to_owned),
        }
    }

    fn link_count(conn: &mut SqliteConnection) -> i64 {
        tag_entities::table.count().get_result(conn).unwrap()
    }

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

    #[test]
    fn create_and_update_round_trip() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);

        let created = create(&mut conn, input("  Garden ", Some(&ids.tools))).unwrap();
        assert_eq!(created.name, "Garden");
        assert_eq!(created.color.as_deref(), Some("#ff0000"));
        assert_eq!(created.parent_id.as_deref(), Some(ids.tools.as_str()));
        assert_eq!(
            get(&mut conn, &created.id).unwrap().as_ref(),
            Some(&created)
        );

        let changes = TagChanges {
            description: Some("outdoor".to_owned()),
            color: None,
            ..input("Yard", None)
        };
        let updated = update(&mut conn, &created.id, changes).unwrap();
        assert_eq!(updated.name, "Yard");
        assert_eq!(updated.description.as_deref(), Some("outdoor"));
        assert_eq!(updated.color, None);
        assert_eq!(updated.parent_id, None);
        assert_eq!(updated.created_at, created.created_at);
        assert!(updated.updated_at >= created.updated_at);

        assert!(create(&mut conn, input(" ", None)).is_err());
        assert!(update(&mut conn, &created.id, input("", None)).is_err());
        let err = update(&mut conn, "missing", input("X", None)).unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");
    }

    #[test]
    fn parent_must_exist_and_not_cycle() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);

        let err = create(&mut conn, input("Orphan", Some("missing"))).unwrap_err();
        assert!(err.to_string().contains("parent"), "{err}");
        let err = update(&mut conn, &ids.tools, input("Tools", Some("missing"))).unwrap_err();
        assert!(err.to_string().contains("parent"), "{err}");

        // Electronics' parent is Tools, so Tools under Electronics is a cycle.
        let err = update(
            &mut conn,
            &ids.tools,
            input("Tools", Some(&ids.electronics)),
        )
        .unwrap_err();
        assert!(err.to_string().contains("cycle"), "{err}");
        let err = update(&mut conn, &ids.tools, input("Tools", Some(&ids.tools))).unwrap_err();
        assert!(err.to_string().contains("cycle"), "{err}");
        assert_eq!(get(&mut conn, &ids.tools).unwrap().unwrap().parent_id, None);

        // Re-parenting down a branch that does not loop back is fine.
        let garden = create(&mut conn, input("Garden", None)).unwrap();
        let moved = update(
            &mut conn,
            &ids.electronics,
            input("Electronics", Some(&garden.id)),
        )
        .unwrap();
        assert_eq!(moved.parent_id.as_deref(), Some(garden.id.as_str()));
    }

    #[test]
    fn delete_cascades_links() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        assert_eq!(link_count(&mut conn), 1);

        delete(&mut conn, &ids.tools).unwrap();

        assert!(get(&mut conn, &ids.tools).unwrap().is_none());
        assert!(for_entity(&mut conn, &ids.drill).unwrap().is_empty());
        assert_eq!(link_count(&mut conn), 0);
        // The child tag survives, detached (FK `ON DELETE SET NULL`).
        let electronics = get(&mut conn, &ids.electronics).unwrap().unwrap();
        assert_eq!(electronics.parent_id, None);
        assert!(delete(&mut conn, &ids.tools).is_err());
    }

    #[test]
    fn color_must_be_a_hex_value() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let colored = |name: &str, color: &str| NewTag {
            color: Some(color.to_owned()),
            ..input(name, None)
        };

        for (name, color) in [
            ("Short", "#abc"),
            ("Long", "#A1b2C3"),
            ("Alpha", "#a1b2c3ff"),
        ] {
            let created = create(&mut conn, colored(name, color)).unwrap();
            assert_eq!(created.color.as_deref(), Some(color));
        }
        let trimmed = create(&mut conn, colored("Padded", " #0f0 ")).unwrap();
        assert_eq!(trimmed.color.as_deref(), Some("#0f0"));

        for bad in [
            "red",
            "#12",
            "#1234",
            "#12345",
            "#1234567",
            "#123456789",
            "#ggg",
            "123456",
            "#fff;background:url(x)",
            "url(https://example.com/x.png)",
        ] {
            let err = create(&mut conn, colored("Bad", bad)).unwrap_err();
            assert_eq!(
                err.to_string(),
                "colour must be a hex value like #a1b2c3",
                "{bad}"
            );
            let err = update(&mut conn, &ids.tools, colored("Tools", bad)).unwrap_err();
            assert_eq!(
                err.to_string(),
                "colour must be a hex value like #a1b2c3",
                "{bad}"
            );
        }
        let tools = get(&mut conn, &ids.tools).unwrap().unwrap();
        assert_eq!(tools.name, "Tools");

        let updated = update(&mut conn, &ids.tools, colored("Tools", "#ABCDEF")).unwrap();
        assert_eq!(updated.color.as_deref(), Some("#ABCDEF"));
    }

    #[test]
    fn blank_optional_text_is_stored_as_none() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let blanks = NewTag {
            description: Some("  ".to_owned()),
            color: Some(String::new()),
            icon: Some(" \t".to_owned()),
            parent_id: Some(" ".to_owned()),
            ..input("Garden", None)
        };
        let created = create(&mut conn, blanks.clone()).unwrap();
        assert_eq!(
            (
                created.description,
                created.color,
                created.icon,
                created.parent_id
            ),
            (None, None, None, None)
        );
        let updated = update(
            &mut conn,
            &ids.electronics,
            NewTag {
                description: Some(" wires ".to_owned()),
                ..blanks
            },
        )
        .unwrap();
        assert_eq!(updated.description.as_deref(), Some("wires"));
        assert_eq!(updated.parent_id, None);
    }
}
