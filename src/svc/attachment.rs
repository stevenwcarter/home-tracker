//! Attachment metadata and stored thumbnails.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, ensure};
use diesel::prelude::*;
use tracing::warn;

use crate::kinds::AttachmentKind;
use crate::models::{Attachment, Entity, Thumbnail};
use crate::schema::{attachments, thumbnails};
use crate::svc::entity;

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

/// Where the bytes of the original with `sha256` live under `data_dir`.
pub fn original_path(data_dir: &Path, sha256: &str) -> PathBuf {
    data_dir.join("originals").join(sha256)
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

/// Stores `thumb` unless a row for its `(attachment_id, size)` already exists.
pub fn insert_thumbnail(conn: &mut SqliteConnection, thumb: &Thumbnail) -> Result<()> {
    diesel::insert_into(thumbnails::table)
        .values(thumb)
        .on_conflict_do_nothing()
        .execute(conn)
        .with_context(|| {
            format!(
                "storing the {}px thumbnail of attachment {:?}",
                thumb.size, thumb.attachment_id
            )
        })?;
    Ok(())
}

/// The attachment with `id`, or a user-readable "not found" error.
fn require(conn: &mut SqliteConnection, id: &str) -> Result<Attachment> {
    get(conn, id)?.ok_or_else(|| anyhow!("attachment not found"))
}

/// Deletes attachment `id`; its thumbnails go with it by foreign key. The
/// original file is removed once the row is gone, when no other attachment
/// shares its hash.
pub fn delete(conn: &mut SqliteConnection, data_dir: &Path, id: &str) -> Result<()> {
    if let Some(sha256) = conn.transaction(|conn| delete_row(conn, id))? {
        remove_original(data_dir, &sha256);
    }
    Ok(())
}

/// Deletes the row of attachment `id` and returns its hash when no other row
/// shares it, meaning its original is now unreferenced. The caller removes
/// that file with [`remove_original`] after its transaction commits.
pub(crate) fn delete_row(conn: &mut SqliteConnection, id: &str) -> Result<Option<String>> {
    let row = require(conn, id)?;
    diesel::delete(attachments::table.find(id))
        .execute(conn)
        .with_context(|| format!("deleting attachment {id:?}"))?;
    let sharing: i64 = attachments::table
        .filter(attachments::sha256.eq(&row.sha256))
        .count()
        .get_result(conn)
        .with_context(|| format!("counting attachments sharing {:?}", row.sha256))?;
    Ok((sharing == 0).then_some(row.sha256))
}

/// Removes the original with `sha256` from `data_dir`. The rows are already
/// committed as deleted, so a failure only leaks disk space: it is logged
/// rather than reported, and an already-missing file is fine.
pub(crate) fn remove_original(data_dir: &Path, sha256: &str) {
    let path = original_path(data_dir, sha256);
    if let Err(err) = fs::remove_file(&path)
        && err.kind() != ErrorKind::NotFound
    {
        warn!(path = %path.display(), %err, "could not remove an unreferenced original");
    }
}

/// Makes photo `attachment_id` the primary photo of its entity, clearing the
/// flag on that entity's other photos, and returns the entity.
pub fn set_primary(conn: &mut SqliteConnection, attachment_id: &str) -> Result<Entity> {
    conn.transaction(|conn| {
        let photo = require(conn, attachment_id)?;
        ensure!(
            photo.kind == AttachmentKind::Photo,
            "only a photo can be the primary photo"
        );
        diesel::update(
            attachments::table
                .filter(attachments::entity_id.eq(&photo.entity_id))
                .filter(attachments::kind.eq(AttachmentKind::Photo)),
        )
        .set(attachments::is_primary.eq(attachments::id.eq(attachment_id)))
        .execute(conn)
        .with_context(|| format!("setting the primary photo of entity {:?}", photo.entity_id))?;
        entity::require(conn, &photo.entity_id)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::db::TestDb;
    use crate::kinds::AttachmentKind;
    use crate::schema::attachments;
    use crate::svc::fixtures::{attachment, seed_sample};

    fn insert(conn: &mut SqliteConnection, row: Attachment) {
        diesel::insert_into(attachments::table)
            .values(row)
            .execute(conn)
            .unwrap();
    }

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

    #[test]
    fn attachment_delete_removes_the_file_only_when_unshared() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(data.path().join("originals")).unwrap();
        let sha = "aa".repeat(32);
        let file = original_path(data.path(), &sha);
        fs::write(&file, b"bytes").unwrap();
        insert(
            &mut conn,
            attachment(
                "a-screws-photo",
                &ids.screws,
                AttachmentKind::Photo,
                true,
                &sha,
                5,
                20,
            ),
        );

        delete(&mut conn, data.path(), &ids.photo).unwrap();
        assert!(get(&mut conn, &ids.photo).unwrap().is_none());
        assert!(thumbnail(&mut conn, &ids.photo, 500).unwrap().is_none());
        assert!(file.exists(), "a shared original must stay");

        delete(&mut conn, data.path(), "a-screws-photo").unwrap();
        assert!(!file.exists(), "an unshared original must go");

        // A missing file is not an error: the row is what matters.
        delete(&mut conn, data.path(), &ids.manual).unwrap();
        assert!(get(&mut conn, &ids.manual).unwrap().is_none());
        let err = delete(&mut conn, data.path(), &ids.manual).unwrap_err();
        assert_eq!(err.to_string(), "attachment not found");
    }

    #[test]
    fn set_primary_moves_the_flag_within_the_entity() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let later = "a-drill-later-photo";
        insert(
            &mut conn,
            attachment(
                later,
                &ids.drill,
                AttachmentKind::Photo,
                false,
                &"cc".repeat(32),
                7,
                20,
            ),
        );
        insert(
            &mut conn,
            attachment(
                "a-screws-photo",
                &ids.screws,
                AttachmentKind::Photo,
                true,
                &"dd".repeat(32),
                7,
                21,
            ),
        );

        let owner = set_primary(&mut conn, later).unwrap();
        assert_eq!(owner.id, ids.drill);
        assert_eq!(
            primary_photo(&mut conn, &ids.drill).unwrap().unwrap().id,
            later
        );
        assert!(!get(&mut conn, &ids.photo).unwrap().unwrap().is_primary);
        assert!(!get(&mut conn, &ids.manual).unwrap().unwrap().is_primary);
        // Another entity's primary photo is untouched.
        assert!(
            get(&mut conn, "a-screws-photo")
                .unwrap()
                .unwrap()
                .is_primary
        );
    }

    #[test]
    fn set_primary_rejects_a_non_photo_or_foreign_attachment() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let err = set_primary(&mut conn, &ids.manual).unwrap_err();
        assert_eq!(err.to_string(), "only a photo can be the primary photo");
        let err = set_primary(&mut conn, "missing").unwrap_err();
        assert_eq!(err.to_string(), "attachment not found");
        assert!(get(&mut conn, &ids.photo).unwrap().unwrap().is_primary);
    }
}
