//! Attachment metadata, stored thumbnails, and the lifetime of originals.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use anyhow::{Context, Result, anyhow, ensure};
use diesel::prelude::*;
use diesel::sql_types::{Binary, Bool, Integer, Text, Timestamp};
use tracing::warn;

use crate::kinds::AttachmentKind;
use crate::models::{Attachment, Entity, Thumbnail};
use crate::schema::{attachments, thumbnails};
use crate::svc::{entity, thumbnail};

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

/// The directory under `data_dir` holding originals by content address.
pub fn originals_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("originals")
}

/// Where the bytes of the original with `sha256` live under `data_dir`.
pub fn original_path(data_dir: &Path, sha256: &str) -> PathBuf {
    originals_dir(data_dir).join(sha256)
}

/// The thumbnail size a URL names when the caller does not pick one.
pub const DEFAULT_THUMB_URL_SIZE: i32 = 500;

/// Where `att`'s original is served. The `?v=` tag changes whenever the bytes
/// do, so clients can cache the URL forever instead of the id.
pub fn original_url(att: &Attachment) -> String {
    format!("/attachments/{}?v={}", att.id, version_tag(&att.sha256))
}

/// Where a thumbnail of at most `size` pixels of `att` is served; `None` for
/// non-images. The size is echoed as given: the HTTP handler rounds it to an
/// allowed size.
pub fn thumbnail_url(att: &Attachment, size: i32) -> Option<String> {
    thumbnail::is_thumbnailable(&att.mime_type).then(|| {
        format!(
            "/attachments/{}/thumb/{size}?v={}",
            att.id,
            version_tag(&att.sha256)
        )
    })
}

/// `att`'s original URL and its thumbnail URL at [`DEFAULT_THUMB_URL_SIZE`]:
/// what the GraphQL `Attachment` reports by default, and what the upload
/// response carries, so the two cannot drift apart.
pub fn attachment_urls(att: &Attachment) -> (String, Option<String>) {
    (
        original_url(att),
        thumbnail_url(att, DEFAULT_THUMB_URL_SIZE),
    )
}

/// A short, stable cache-busting tag for `sha256`: its first 12 hex characters
/// (the whole string if shorter), which is already plenty of entropy per
/// attachment id to change whenever the underlying bytes do.
fn version_tag(sha256: &str) -> &str {
    &sha256[..sha256.len().min(12)]
}

/// The stored `size` thumbnail of the original with `sha256`; no fallback
/// to another size.
pub fn thumbnail(
    conn: &mut SqliteConnection,
    sha256: &str,
    size: i32,
) -> Result<Option<Thumbnail>> {
    thumbnails::table
        .find((sha256, size))
        .select(Thumbnail::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading the {size}px thumbnail of {sha256:?}"))
}

/// SQL that is true while any attachment or staged ingest photo references
/// the sha256 bound as `?1`. [`is_shared`] and [`insert_thumbnail`] both use
/// it, so what counts as a sharer cannot drift between them.
const SHARER_EXISTS: &str = "(EXISTS (SELECT 1 FROM attachments WHERE sha256 = ?1) \
     OR EXISTS (SELECT 1 FROM ingest_photos WHERE sha256 = ?1))";

/// Stores `thumb` unless a row for its `(sha256, size)` already exists: the
/// same bytes make the same thumbnail, so the first one stored wins. Nothing
/// is stored once no row shares the sha: a thumbnail generated while the
/// last sharer was deleted would otherwise outlive [`remove_original`]'s
/// cleanup. The check and the insert are one statement, so no delete can
/// commit between them.
pub fn insert_thumbnail(conn: &mut SqliteConnection, thumb: &Thumbnail) -> Result<()> {
    diesel::sql_query(format!(
        "INSERT INTO thumbnails (sha256, size, mime_type, width, height, data, created_at) \
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7 WHERE {SHARER_EXISTS} \
         ON CONFLICT DO NOTHING"
    ))
    .bind::<Text, _>(&thumb.sha256)
    .bind::<Integer, _>(thumb.size)
    .bind::<Text, _>(&thumb.mime_type)
    .bind::<Integer, _>(thumb.width)
    .bind::<Integer, _>(thumb.height)
    .bind::<Binary, _>(&thumb.data)
    .bind::<Timestamp, _>(thumb.created_at)
    .execute(conn)
    .with_context(|| {
        format!(
            "storing the {}px thumbnail of {:?}",
            thumb.size, thumb.sha256
        )
    })?;
    Ok(())
}

/// Deletes every stored thumbnail of the original with `sha256`.
pub fn delete_thumbnails(conn: &mut SqliteConnection, sha256: &str) -> Result<()> {
    diesel::delete(thumbnails::table.filter(thumbnails::sha256.eq(sha256)))
        .execute(conn)
        .with_context(|| format!("deleting the thumbnails of {sha256:?}"))?;
    Ok(())
}

/// The attachment with `id`, or a user-readable "not found" error.
fn require(conn: &mut SqliteConnection, id: &str) -> Result<Attachment> {
    get(conn, id)?.ok_or_else(|| anyhow!("attachment not found"))
}

/// Deletes attachment `id`. When it was its entity's primary photo, the
/// earliest remaining photo that can be thumbnailed takes over, just as the
/// first photo uploaded becomes primary. The original file and its
/// thumbnails are removed once the row is gone, when no other attachment or
/// staged photo shares its hash.
pub fn delete(conn: &mut SqliteConnection, data_dir: &Path, id: &str) -> Result<()> {
    let orphan = conn.transaction(|conn| {
        let row = require(conn, id)?;
        let orphan = delete_row(conn, &row)?;
        if row.is_primary && row.kind == AttachmentKind::Photo {
            promote_earliest_photo(conn, &row.entity_id)?;
        }
        Ok::<_, anyhow::Error>(orphan)
    })?;
    if let Some(sha256) = orphan {
        remove_original(conn, data_dir, &sha256);
    }
    Ok(())
}

/// Flags the earliest photo of `entity_id` (by creation, then id) that can be
/// thumbnailed as primary. A photo that cannot (an imported HEIC, say) would
/// be a primary photo nothing can show, so it is passed over.
fn promote_earliest_photo(conn: &mut SqliteConnection, entity_id: &str) -> Result<()> {
    let photos: Vec<(String, String)> = attachments::table
        .filter(attachments::entity_id.eq(entity_id))
        .filter(attachments::kind.eq(AttachmentKind::Photo))
        .order((attachments::created_at.asc(), attachments::id.asc()))
        .select((attachments::id, attachments::mime_type))
        .load(conn)
        .with_context(|| format!("finding the photos of entity {entity_id:?}"))?;
    let earliest = photos
        .into_iter()
        .find_map(|(id, mime)| thumbnail::is_thumbnailable(&mime).then_some(id));
    if let Some(earliest) = earliest {
        diesel::update(attachments::table.find(&earliest))
            .set(attachments::is_primary.eq(true))
            .execute(conn)
            .with_context(|| format!("promoting photo {earliest:?} to primary"))?;
    }
    Ok(())
}

/// Deletes `row` and returns its hash when no other row shares it, meaning
/// its original may now be unreferenced. The caller passes that hash to
/// [`remove_original`] after its transaction commits.
pub(crate) fn delete_row(conn: &mut SqliteConnection, row: &Attachment) -> Result<Option<String>> {
    diesel::delete(attachments::table.find(&row.id))
        .execute(conn)
        .with_context(|| format!("deleting attachment {:?}", row.id))?;
    Ok((!is_shared(conn, &row.sha256)?).then(|| row.sha256.clone()))
}

/// Whether any attachment or staged ingest photo references the original
/// with `sha256`.
fn is_shared(conn: &mut SqliteConnection, sha256: &str) -> Result<bool> {
    #[derive(QueryableByName)]
    struct Shared {
        #[diesel(sql_type = Bool)]
        shared: bool,
    }
    diesel::sql_query(format!("SELECT {SHARER_EXISTS} AS shared"))
        .bind::<Text, _>(sha256)
        .get_result::<Shared>(conn)
        .map(|row| row.shared)
        .with_context(|| format!("counting rows sharing {sha256:?}"))
}

/// Originals an upload is placing, by path, with how many uploads are placing
/// each: identical bytes can arrive twice at once. Process-wide because the
/// app is a single process and originals are shared by every request.
static PLACING: LazyLock<Mutex<HashMap<PathBuf, usize>>> = LazyLock::new(Mutex::default);

fn placing() -> MutexGuard<'static, HashMap<PathBuf, usize>> {
    PLACING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An upload's claim on an original, from before it checks for or renames
/// the file until after its row commits. While any claim is held,
/// [`remove_original`] leaves that file alone, so a delete of the last other
/// sharer cannot remove bytes the upload is about to reference.
pub(crate) struct PlacingOriginal {
    path: PathBuf,
}

impl PlacingOriginal {
    /// Claims the original at `path` until the returned guard is dropped.
    pub(crate) fn claim(path: PathBuf) -> Self {
        *placing().entry(path.clone()).or_default() += 1;
        Self { path }
    }
}

impl Drop for PlacingOriginal {
    fn drop(&mut self) {
        let mut placing = placing();
        if let Some(count) = placing.get_mut(&self.path) {
            *count -= 1;
            if *count == 0 {
                placing.remove(&self.path);
            }
        }
    }
}

/// Removes the original with `sha256` from `data_dir`, and its thumbnail
/// rows, unless an upload is placing it or an attachment or staged ingest
/// photo references it. Both are checked under the placing lock, so an upload
/// that committed its row after the caller's own sharer count still keeps its
/// file and thumbnails. The caller's rows are already committed as deleted, so a
/// failure only leaks space: it is logged rather than reported, and an
/// already-missing file is fine.
pub(crate) fn remove_original(conn: &mut SqliteConnection, data_dir: &Path, sha256: &str) {
    let path = original_path(data_dir, sha256);
    // PLACING is held across the COUNT, the thumbnail-row delete and the file
    // removal on purpose: a claim taken after this check cannot slip between
    // it and the removal. Claims never wait on the database, so this cannot
    // deadlock; the delete is one statement on an indexed column, so the
    // hold stays brief and only briefly serialises claims.
    let placing = placing();
    if placing.contains_key(&path) {
        return;
    }
    match is_shared(conn, sha256) {
        Ok(false) => {}
        Ok(true) => return,
        Err(err) => {
            warn!(%sha256, "not removing an original: {err:#}");
            return;
        }
    }
    if let Err(err) = delete_thumbnails(conn, sha256) {
        warn!(%sha256, "could not remove the thumbnails of an unreferenced original: {err:#}");
    }
    if let Err(err) = fs::remove_file(&path)
        && err.kind() != ErrorKind::NotFound
    {
        warn!(path = %path.display(), %err, "could not remove an unreferenced original");
    }
    drop(placing);
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
    use crate::svc::fixtures::{
        SampleIds, at, attachment, ingest_batch, ingest_photo, seed_sample,
    };
    use crate::svc::ingest;

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
        let thumb = thumbnail(&mut conn, &ids.photo_sha256, 500)
            .unwrap()
            .unwrap();
        assert_eq!(thumb.width, 4);
        assert_eq!(thumb.data.len(), 8);
        assert!(
            thumbnail(&mut conn, &ids.photo_sha256, 300)
                .unwrap()
                .is_none()
        );
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
    fn deleting_the_last_sharer_removes_its_thumbnails() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let (data, file) = data_with_drill_photo();
        let sha = &ids.photo_sha256;
        insert(
            &mut conn,
            attachment(
                "a-screws-photo",
                &ids.screws,
                AttachmentKind::Photo,
                true,
                sha,
                5,
                20,
            ),
        );
        let sizes = |conn: &mut SqliteConnection| -> Vec<i32> {
            thumbnails::table
                .filter(thumbnails::sha256.eq(sha))
                .select(thumbnails::size)
                .load(conn)
                .unwrap()
        };

        delete(&mut conn, data.path(), &ids.photo).unwrap();
        assert_eq!(sizes(&mut conn), [500], "a shared thumbnail must stay");
        assert!(file.exists());

        delete(&mut conn, data.path(), "a-screws-photo").unwrap();
        assert!(sizes(&mut conn).is_empty(), "an unshared thumbnail must go");
        assert!(!file.exists());
    }

    #[test]
    fn a_sha_shared_by_an_attachment_and_an_ingest_photo_survives_either_delete() {
        // Review focus 1: either side keeps the file while the other holds it.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let (data, photo_file) = data_with_drill_photo();
        let manual_sha = "bb".repeat(32);
        let manual_file = original_path(data.path(), &manual_sha);
        fs::write(&manual_file, b"manual").unwrap();
        let (_, item) = ingest_batch(&mut conn, None);
        let staged_photo = ingest_photo(&mut conn, &item.id, &ids.photo_sha256, "image/jpeg");
        let staged_manual = ingest_photo(&mut conn, &item.id, &manual_sha, "image/jpeg");
        let has_thumbnail = |conn: &mut SqliteConnection| {
            thumbnail(conn, &ids.photo_sha256, 500).unwrap().is_some()
        };

        // The attachment goes first: the staged photo keeps file and thumbnail.
        delete(&mut conn, data.path(), &ids.photo).unwrap();
        assert!(photo_file.exists());
        assert!(has_thumbnail(&mut conn));
        ingest::remove_photo(&mut conn, data.path(), &staged_photo.id).unwrap();
        assert!(!photo_file.exists());
        assert!(!has_thumbnail(&mut conn));

        // The staged photo goes first: the attachment keeps the file.
        ingest::remove_photo(&mut conn, data.path(), &staged_manual.id).unwrap();
        assert!(manual_file.exists());
        delete(&mut conn, data.path(), &ids.manual).unwrap();
        assert!(!manual_file.exists());
    }

    #[test]
    fn insert_thumbnail_refuses_a_row_with_no_sharer() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let data = tempfile::tempdir().unwrap();
        let sha = "cc".repeat(32);
        let row = |sha256: &str, size: i32| Thumbnail {
            sha256: sha256.to_owned(),
            size,
            mime_type: "image/webp".to_owned(),
            width: 4,
            height: 3,
            data: vec![7; 8],
            created_at: at(20),
        };

        insert_thumbnail(&mut conn, &row(&sha, 300)).unwrap();
        assert!(thumbnail(&mut conn, &sha, 300).unwrap().is_none());

        // An attachment and a staged photo are each a sharer.
        insert_thumbnail(&mut conn, &row(&ids.photo_sha256, 300)).unwrap();
        assert_eq!(
            thumbnail(&mut conn, &ids.photo_sha256, 300).unwrap(),
            Some(row(&ids.photo_sha256, 300))
        );
        let (_, item) = ingest_batch(&mut conn, None);
        let staged = ingest_photo(&mut conn, &item.id, &sha, "image/jpeg");
        insert_thumbnail(&mut conn, &row(&sha, 300)).unwrap();
        assert_eq!(
            thumbnail(&mut conn, &sha, 300).unwrap(),
            Some(row(&sha, 300))
        );
        // A second insert of the same key keeps the first row.
        insert_thumbnail(
            &mut conn,
            &Thumbnail {
                width: 9,
                ..row(&sha, 300)
            },
        )
        .unwrap();
        assert_eq!(thumbnail(&mut conn, &sha, 300).unwrap().unwrap().width, 4);

        // Once the sharer is gone, a late insert (a thumbnail generated while
        // the delete ran) must not leave an orphan row behind.
        ingest::remove_photo(&mut conn, data.path(), &staged.id).unwrap();
        assert!(thumbnail(&mut conn, &sha, 300).unwrap().is_none());
        insert_thumbnail(&mut conn, &row(&sha, 1200)).unwrap();
        assert!(thumbnail(&mut conn, &sha, 1200).unwrap().is_none());
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

    /// Every attachment of `entity_id` as `(id, is_primary)`, in id order.
    fn flags(conn: &mut SqliteConnection, entity_id: &str) -> Vec<(String, bool)> {
        attachments::table
            .filter(attachments::entity_id.eq(entity_id))
            .order(attachments::id.asc())
            .select((attachments::id, attachments::is_primary))
            .load(conn)
            .unwrap()
    }

    /// Two more drill photos besides the seeded primary one: `a-drill-early`
    /// (created before) and `a-drill-late` (created after).
    fn add_drill_photos(conn: &mut SqliteConnection, ids: &SampleIds) {
        for (id, seq) in [("a-drill-late", 30), ("a-drill-early", 1)] {
            insert(
                conn,
                attachment(
                    id,
                    &ids.drill,
                    AttachmentKind::Photo,
                    false,
                    &"cc".repeat(32),
                    7,
                    seq,
                ),
            );
        }
    }

    #[test]
    fn deleting_the_primary_photo_promotes_the_earliest_remaining_photo() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        add_drill_photos(&mut conn, &ids);
        let data = tempfile::tempdir().unwrap();

        delete(&mut conn, data.path(), &ids.photo).unwrap();

        assert_eq!(
            flags(&mut conn, &ids.drill),
            [
                ("a-drill-early".to_owned(), true),
                ("a-drill-late".to_owned(), false),
                (ids.manual.clone(), false),
            ]
        );
        assert_eq!(
            primary_photo(&mut conn, &ids.drill).unwrap().unwrap().id,
            "a-drill-early"
        );
    }

    #[test]
    fn promotion_skips_photos_that_cannot_be_thumbnailed() {
        // An imported HEIC is a photo row, but it would be an invisible primary.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        add_drill_photos(&mut conn, &ids);
        diesel::update(attachments::table.find("a-drill-early"))
            .set(attachments::mime_type.eq("image/heic"))
            .execute(&mut conn)
            .unwrap();
        let data = tempfile::tempdir().unwrap();

        delete(&mut conn, data.path(), &ids.photo).unwrap();

        assert_eq!(
            flags(&mut conn, &ids.drill),
            [
                ("a-drill-early".to_owned(), false),
                ("a-drill-late".to_owned(), true),
                (ids.manual.clone(), false),
            ]
        );
    }

    #[test]
    fn deleting_a_non_primary_photo_changes_no_flags() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        add_drill_photos(&mut conn, &ids);
        let data = tempfile::tempdir().unwrap();
        let mut expected = flags(&mut conn, &ids.drill);
        expected.retain(|(id, _)| id != "a-drill-early");

        delete(&mut conn, data.path(), "a-drill-early").unwrap();

        assert_eq!(flags(&mut conn, &ids.drill), expected);
    }

    #[test]
    fn deleting_the_last_photo_leaves_no_primary() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let data = tempfile::tempdir().unwrap();

        delete(&mut conn, data.path(), &ids.photo).unwrap();

        // The manual is not a photo, so it must not inherit the flag.
        assert_eq!(flags(&mut conn, &ids.drill), [(ids.manual.clone(), false)]);
        assert!(primary_photo(&mut conn, &ids.drill).unwrap().is_none());
    }

    /// A data dir holding the seeded drill photo's original (`aa…`).
    fn data_with_drill_photo() -> (tempfile::TempDir, PathBuf) {
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(data.path().join("originals")).unwrap();
        let file = original_path(data.path(), &"aa".repeat(32));
        fs::write(&file, b"bytes").unwrap();
        (data, file)
    }

    #[test]
    fn an_original_being_placed_by_an_upload_survives_a_delete() {
        // Carry-over (a): an upload of identical bytes racing the delete of
        // their last other sharer must not lose the file.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let (data, file) = data_with_drill_photo();

        let claim = PlacingOriginal::claim(file.clone());
        delete(&mut conn, data.path(), &ids.photo).unwrap();
        assert!(file.exists(), "a file an upload is placing must stay");
        drop(claim);

        let other = original_path(data.path(), &"bb".repeat(32));
        fs::write(&other, b"manual").unwrap();
        delete(&mut conn, data.path(), &ids.manual).unwrap();
        assert!(
            !other.exists(),
            "an unclaimed unreferenced original must go"
        );
    }

    #[test]
    fn overlapping_claims_on_one_original_all_have_to_end() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (data, file) = data_with_drill_photo();
        diesel::delete(attachments::table)
            .execute(&mut conn)
            .unwrap();
        let sha = "aa".repeat(32);

        let first = PlacingOriginal::claim(file.clone());
        let second = PlacingOriginal::claim(file.clone());
        drop(first);
        remove_original(&mut conn, data.path(), &sha);
        assert!(file.exists(), "one upload is still placing the file");
        drop(second);
        remove_original(&mut conn, data.path(), &sha);
        assert!(!file.exists());
    }

    #[test]
    fn remove_original_rechecks_sharers() {
        // A row committed after the caller's own sharer count keeps the file.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let (data, file) = data_with_drill_photo();
        let row = get(&mut conn, &ids.photo).unwrap().unwrap();
        let orphan = delete_row(&mut conn, &row).unwrap();
        assert_eq!(orphan.as_deref(), Some(row.sha256.as_str()));
        insert(
            &mut conn,
            attachment(
                "a-late-sharer",
                &ids.screws,
                AttachmentKind::Photo,
                true,
                &row.sha256,
                5,
                40,
            ),
        );

        remove_original(&mut conn, data.path(), &row.sha256);

        assert!(file.exists());
    }
}
