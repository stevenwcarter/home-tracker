//! Attachment metadata and stored thumbnails.

use anyhow::{Context, Result};
use diesel::prelude::*;

use crate::kinds::AttachmentKind;
use crate::models::{Attachment, Thumbnail};
use crate::schema::{attachments, thumbnails};

/// The attachments on `entity_id`: the primary one first, then oldest first.
pub fn for_entity(conn: &mut SqliteConnection, entity_id: &str) -> Result<Vec<Attachment>> {
    attachments::table
        .filter(attachments::entity_id.eq(entity_id))
        .order((
            attachments::is_primary.desc(),
            attachments::created_at.asc(),
            attachments::id.asc(),
        ))
        .select(Attachment::as_select())
        .load(conn)
        .with_context(|| format!("loading attachments of entity {entity_id:?}"))
}

/// The photo to show for `entity_id`: the flagged primary photo, else the
/// earliest photo, else `None`.
pub fn primary_photo(conn: &mut SqliteConnection, entity_id: &str) -> Result<Option<Attachment>> {
    let photos = attachments::table
        .filter(attachments::entity_id.eq(entity_id))
        .filter(attachments::kind.eq(AttachmentKind::Photo));
    let flagged = photos
        .filter(attachments::is_primary.eq(true))
        .select(Attachment::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading the primary photo of entity {entity_id:?}"))?;
    if flagged.is_some() {
        return Ok(flagged);
    }
    photos
        .order((attachments::created_at.asc(), attachments::id.asc()))
        .select(Attachment::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading the earliest photo of entity {entity_id:?}"))
}

/// The attachment with `id`, if any.
pub fn get(conn: &mut SqliteConnection, id: &str) -> Result<Option<Attachment>> {
    attachments::table
        .find(id)
        .select(Attachment::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading attachment {id:?}"))
}

/// The stored thumbnail of exactly `size`; no fallback to another size.
pub fn thumbnail(
    conn: &mut SqliteConnection,
    attachment_id: &str,
    size: i32,
) -> Result<Option<Thumbnail>> {
    thumbnails::table
        .find((attachment_id, size))
        .select(Thumbnail::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading the {size}px thumbnail of attachment {attachment_id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::kinds::AttachmentKind;
    use crate::schema::attachments;
    use crate::svc::fixtures::{attachment, seed_sample};

    #[test]
    fn for_entity_lists_primary_first() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let listed: Vec<String> = for_entity(&mut conn, &ids.drill)
            .unwrap()
            .into_iter()
            .map(|a| a.id)
            .collect();
        assert_eq!(listed, [ids.photo.as_str(), ids.manual.as_str()]);
        let manual = get(&mut conn, &ids.manual).unwrap().unwrap();
        assert_eq!(manual.kind, AttachmentKind::Manual);
    }

    #[test]
    fn primary_photo_prefers_the_flag_then_the_earliest_photo() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let flagged = primary_photo(&mut conn, &ids.drill).unwrap().unwrap();
        assert_eq!(flagged.id, ids.photo);

        diesel::update(attachments::table.find(&ids.photo))
            .set(attachments::is_primary.eq(false))
            .execute(&mut conn)
            .unwrap();
        diesel::insert_into(attachments::table)
            .values(attachment(
                "a-drill-later-photo",
                &ids.drill,
                AttachmentKind::Photo,
                false,
                &"cc".repeat(32),
                7,
                100,
            ))
            .execute(&mut conn)
            .unwrap();
        let earliest = primary_photo(&mut conn, &ids.drill).unwrap().unwrap();
        assert_eq!(earliest.id, ids.photo);
        assert!(primary_photo(&mut conn, &ids.screws).unwrap().is_none());
    }

    #[test]
    fn thumbnail_returns_the_stored_size_only() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let thumb = thumbnail(&mut conn, &ids.photo, 500).unwrap().unwrap();
        assert_eq!(thumb.width, 4);
        assert_eq!(thumb.data.len(), 8);
        assert!(thumbnail(&mut conn, &ids.photo, 300).unwrap().is_none());
    }
}
