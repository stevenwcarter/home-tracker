//! Storing an uploaded photo: stream it to a temp file while hashing, decide
//! its type from its bytes, check its header decodes within the thumbnailer's
//! limits, move it to its content address (or drop it when those bytes are
//! already stored), make its smallest thumbnail (which decodes every pixel,
//! so a damaged body is refused too), and insert its row and that thumbnail
//! in one transaction: an attachment under the primary rule ([`store`]) or a
//! staged ingest photo ([`store_ingest_photo`]).

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, anyhow};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use image::{ImageDecoder, ImageReader};
use sha2::{Digest, Sha256};
use tempfile::TempPath;
use tracing::info;
use uuid::Uuid;

use crate::kinds::{AttachmentKind, IngestBatchStatus};
use crate::models::{Attachment, IngestPhoto, Thumbnail};
use crate::schema::attachments;
use crate::svc::ingest::{self, IngestError};
use crate::svc::sniff::{self, ImageFormat, SNIFF_LEN};
use crate::svc::thumbnail::ThumbSize;
use crate::svc::{attachment, entity, thumbnail};

/// What an upload is for: the row an [`UploadError::NotFound`] could not find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadTarget {
    Entity,
    IngestItem,
}

impl fmt::Display for UploadTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Entity => "entity",
            Self::IngestItem => "ingest item",
        })
    }
}

/// Why an upload was not stored. Each variant maps to one HTTP status.
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("{0} not found")]
    NotFound(UploadTarget),
    /// The ingest item's batch has been submitted.
    #[error("This batch is no longer collecting photos")]
    Conflict,
    #[error("only JPEG, PNG, GIF and WebP images are supported")]
    Unsupported,
    #[error("the file is not a readable image")]
    Unreadable,
    #[error(transparent)]
    Io(#[from] anyhow::Error),
}

impl From<diesel::result::Error> for UploadError {
    fn from(err: diesel::result::Error) -> Self {
        Self::Io(err.into())
    }
}

/// An upload being received: a temp file under `originals/` that is hashed
/// as it is written. Dropped before [`TempUpload::finish`], it removes the
/// temp file, so an aborted request leaves nothing behind.
///
/// # Blocking
///
/// Every method does synchronous file I/O: call them from `spawn_blocking`
/// (or accept a short block per chunk).
pub struct TempUpload {
    file: BufWriter<File>,
    temp: TempPath,
    hasher: Sha256,
    size_bytes: u64,
    head: [u8; SNIFF_LEN],
}

impl TempUpload {
    /// Opens `originals_dir/.upload-<uuid v7>` for writing, creating the
    /// directory if needed. The dot prefix keeps it out of any sha256 name.
    pub fn create(originals_dir: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(originals_dir).context("creating the originals directory")?;
        // The guard exists before the file does, so no failure can leak it.
        let temp =
            TempPath::try_from_path(originals_dir.join(format!(".upload-{}", Uuid::now_v7())))
                .context("naming the upload's temp file")?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .context("creating the upload's temp file")?;
        Ok(Self {
            file: BufWriter::new(file),
            temp,
            hasher: Sha256::new(),
            size_bytes: 0,
            head: [0; SNIFF_LEN],
        })
    }

    /// Appends `chunk` to the temp file and the running hash.
    pub fn write(&mut self, chunk: &[u8]) -> anyhow::Result<()> {
        self.file
            .write_all(chunk)
            .context("writing the upload's temp file")?;
        self.hasher.update(chunk);
        // Fill whatever part of the head is still missing from this chunk.
        let filled = usize::try_from(self.size_bytes).map_or(SNIFF_LEN, |n| n.min(SNIFF_LEN));
        let take = (SNIFF_LEN - filled).min(chunk.len());
        self.head[filled..filled + take].copy_from_slice(&chunk[..take]);
        self.size_bytes += u64::try_from(chunk.len()).context("chunk too large")?;
        Ok(())
    }

    /// Flushes and syncs the temp file and hands it over as a [`Staged`] upload.
    pub fn finish(self) -> anyhow::Result<Staged> {
        let file = self
            .file
            .into_inner()
            .map_err(|err| err.into_error())
            .context("flushing the upload's temp file")?;
        file.sync_all().context("syncing the upload's temp file")?;
        Ok(Staged {
            temp: self.temp,
            sha256: hex::encode(self.hasher.finalize()),
            size_bytes: self.size_bytes,
            head: self.head,
        })
    }
}

/// A fully received upload, still in its temp file. Dropping it without
/// [`store`] removes the temp file.
pub struct Staged {
    temp: TempPath,
    sha256: String,
    size_bytes: u64,
    head: [u8; SNIFF_LEN],
}

/// The attachment an upload became.
#[derive(Debug)]
pub struct StoredUpload {
    pub attachment: Attachment,
}

/// Stores `staged` as a photo of `entity_id` titled after `filename` (the
/// client's name for it, advisory only). The first photo of an entity
/// becomes its primary photo, as does any photo uploaded with `primary`.
/// Its [`ThumbSize::SMALLEST`] thumbnail is generated and stored with it, so
/// an image whose pixels do not decode is [`UploadError::Unreadable`]. On any
/// failure the temp file is removed, and so is a newly placed original that
/// no attachment references.
///
/// # Blocking
///
/// Reads and decodes the file, renames it and queries SQLite synchronously:
/// call it from `spawn_blocking`.
pub fn store(
    conn: &mut SqliteConnection,
    data_dir: &Path,
    entity_id: &str,
    staged: Staged,
    filename: Option<&str>,
    primary: bool,
) -> Result<StoredUpload, UploadError> {
    // Refused before any file work; `commit` checks again inside its
    // transaction in case the entity is deleted meanwhile.
    if entity::get(conn, entity_id)?.is_none() {
        return Err(UploadError::NotFound(UploadTarget::Entity));
    }
    let placed = place(data_dir, staged)?;
    commit(conn, data_dir, entity_id, placed, filename, primary)
}

/// Stages `staged` as the next photo of ingest item `item_id`, titled after
/// `filename`, through the same checks, content address and
/// [`ThumbSize::SMALLEST`] thumbnail as [`store`]. An unknown item is
/// [`UploadError::NotFound`]; an item whose batch is no longer collecting is
/// [`UploadError::Conflict`]. On any failure the temp file is removed, and so
/// is a newly placed original that nothing references.
///
/// # Blocking
///
/// As [`store`]: call it from `spawn_blocking`.
pub fn store_ingest_photo(
    conn: &mut SqliteConnection,
    data_dir: &Path,
    item_id: &str,
    staged: Staged,
    filename: Option<&str>,
) -> Result<IngestPhoto, UploadError> {
    // Refused before any file work; the insert checks again inside its
    // transaction in case the batch is submitted meanwhile.
    require_collecting_item(conn, item_id)?;
    let placed = place(data_dir, staged)?;
    commit_ingest_photo(conn, data_dir, item_id, placed, filename)
}

/// `Ok` when ingest item `item_id` exists and its batch is collecting.
fn require_collecting_item(conn: &mut SqliteConnection, item_id: &str) -> Result<(), UploadError> {
    let item =
        ingest::get_item(conn, item_id)?.ok_or(UploadError::NotFound(UploadTarget::IngestItem))?;
    match ingest::get_batch(conn, &item.batch_id)? {
        Some(batch) if batch.status == IngestBatchStatus::Collecting => Ok(()),
        Some(_) => Err(UploadError::Conflict),
        None => Err(UploadError::NotFound(UploadTarget::IngestItem)),
    }
}

/// An upload whose bytes are validated and stored at their content address,
/// claimed until [`commit`] has inserted its row or given up.
struct Placed {
    claim: attachment::PlacingOriginal,
    format: ImageFormat,
    sha256: String,
    size_bytes: i64,
}

/// Validates `staged` and moves it to its content address, or drops it when
/// those bytes are already stored. The claim is taken before looking for the
/// file, so a concurrent delete of the last other sharer leaves it in place.
fn place(data_dir: &Path, staged: Staged) -> Result<Placed, UploadError> {
    let format = staged.format().ok_or(UploadError::Unsupported)?;
    staged.check_header(format)?;
    let size_bytes = i64::try_from(staged.size_bytes).context("upload too large")?;
    let original = attachment::original_path(data_dir, &staged.sha256);
    let claim = attachment::PlacingOriginal::claim(original.clone());
    if !original.exists() {
        staged
            .temp
            .persist(&original)
            .map_err(|err| anyhow!(err.error).context("moving the upload into place"))?;
    }
    // Otherwise these bytes are already stored, and the unmoved temp file is
    // removed when the rest of `staged` drops at return.
    Ok(Placed {
        claim,
        format,
        sha256: staged.sha256,
        size_bytes,
    })
}

/// Inserts `placed` as a photo attachment of `entity_id` under the primary
/// rule, through [`commit_placed`].
fn commit(
    conn: &mut SqliteConnection,
    data_dir: &Path,
    entity_id: &str,
    placed: Placed,
    filename: Option<&str>,
    primary: bool,
) -> Result<StoredUpload, UploadError> {
    let now = Utc::now().naive_utc();
    let row = Attachment {
        id: Uuid::now_v7().to_string(),
        entity_id: entity_id.to_owned(),
        kind: AttachmentKind::Photo,
        is_primary: false,
        title: clean_title(filename, placed.format.extension()),
        mime_type: placed.format.mime().to_owned(),
        sha256: placed.sha256.clone(),
        size_bytes: placed.size_bytes,
        created_at: now,
        updated_at: now,
    };
    commit_placed(conn, data_dir, placed, now, |conn, thumb| {
        insert_photo(conn, row, thumb, primary)?.ok_or(UploadError::NotFound(UploadTarget::Entity))
    })
    .map(|attachment| StoredUpload { attachment })
}

/// Inserts `placed` as the next staged photo of ingest item `item_id`,
/// through [`commit_placed`]. The ingest row goes in first, so the thumbnail
/// has the sharer [`attachment::insert_thumbnail`] requires.
fn commit_ingest_photo(
    conn: &mut SqliteConnection,
    data_dir: &Path,
    item_id: &str,
    placed: Placed,
    filename: Option<&str>,
) -> Result<IngestPhoto, UploadError> {
    let title = clean_title(filename, placed.format.extension());
    let (sha256, mime_type, size_bytes) = (
        placed.sha256.clone(),
        placed.format.mime(),
        placed.size_bytes,
    );
    commit_placed(
        conn,
        data_dir,
        placed,
        Utc::now().naive_utc(),
        |conn, thumb| {
            let photo =
                ingest::insert_photo_row(conn, item_id, &sha256, mime_type, size_bytes, &title)
                    .map_err(ingest_refusal)?;
            attachment::insert_thumbnail(conn, thumb)?;
            Ok(photo)
        },
    )
}

/// An [`ingest::insert_photo_row`] failure as an upload error: its refusals
/// keep their status, anything else is [`UploadError::Io`].
fn ingest_refusal(err: anyhow::Error) -> UploadError {
    match err.downcast_ref::<IngestError>() {
        Some(IngestError::NotFound(_)) => UploadError::NotFound(UploadTarget::IngestItem),
        Some(IngestError::NotCollecting) => UploadError::Conflict,
        _ => UploadError::Io(err),
    }
}

/// Makes the smallest thumbnail of `placed` (stamped `now`) and runs
/// `insert` with it in one immediate transaction, then releases the claim.
/// On failure, including an original whose pixels do not decode, the
/// original is removed unless something else references it.
fn commit_placed<T>(
    conn: &mut SqliteConnection,
    data_dir: &Path,
    placed: Placed,
    now: NaiveDateTime,
    insert: impl FnOnce(&mut SqliteConnection, &Thumbnail) -> Result<T, UploadError>,
) -> Result<T, UploadError> {
    // Decoded before the transaction, so the write lock is not held for it.
    let inserted = smallest_thumbnail(data_dir, &placed.sha256, now).and_then(|thumb| {
        // Immediate, so simultaneous first uploads to one entity queue for
        // the write lock instead of failing to upgrade a read transaction.
        conn.immediate_transaction(|conn| insert(conn, &thumb))
    });
    drop(placed.claim);
    inserted.inspect_err(|_| attachment::remove_original(conn, data_dir, &placed.sha256))
}

/// The [`ThumbSize::SMALLEST`] thumbnail of the placed original `sha256`,
/// stamped `created_at`, or [`UploadError::Unreadable`] when its pixels do
/// not decode.
fn smallest_thumbnail(
    data_dir: &Path,
    sha256: &str,
    created_at: NaiveDateTime,
) -> Result<Thumbnail, UploadError> {
    let original = fs::read(attachment::original_path(data_dir, sha256))
        .context("reading the placed upload")?;
    let generated =
        thumbnail::generate_bytes(&original, ThumbSize::SMALLEST.get()).map_err(|err| {
            let reason = format!("{err:#}");
            info!(%sha256, %reason, "refusing an unreadable upload");
            UploadError::Unreadable
        })?;
    Ok(generated.into_row(sha256.to_owned(), ThumbSize::SMALLEST, created_at)?)
}

/// Inserts photo `row` and its `thumb`, making the photo its entity's primary
/// photo when asked to or when the entity has no photo yet. `None` when the
/// entity no longer exists.
fn insert_photo(
    conn: &mut SqliteConnection,
    mut row: Attachment,
    thumb: &Thumbnail,
    primary: bool,
) -> anyhow::Result<Option<Attachment>> {
    if entity::get(conn, &row.entity_id)?.is_none() {
        return Ok(None);
    }
    let has_photo = diesel::select(diesel::dsl::exists(
        attachments::table
            .filter(attachments::entity_id.eq(&row.entity_id))
            .filter(attachments::kind.eq(AttachmentKind::Photo)),
    ))
    .get_result::<bool>(conn)
    .with_context(|| format!("looking for photos of entity {:?}", row.entity_id))?;
    diesel::insert_into(attachments::table)
        .values(&row)
        .execute(conn)
        .with_context(|| format!("storing an upload for entity {:?}", row.entity_id))?;
    attachment::insert_thumbnail(conn, thumb)?;
    if primary || !has_photo {
        attachment::set_primary(conn, &row.id)?;
        row.is_primary = true;
    }
    Ok(Some(row))
}

impl Staged {
    /// The format the upload's bytes declare, if it is one we accept.
    fn format(&self) -> Option<ImageFormat> {
        let len = usize::try_from(self.size_bytes).map_or(SNIFF_LEN, |n| n.min(SNIFF_LEN));
        sniff::sniff(&self.head[..len])
    }

    /// Reads the image header as the sniffed `format`, refusing one that does
    /// not decode or whose dimensions or decode buffer exceed the
    /// thumbnailer's limits. No pixels are decoded here: this is the cheap
    /// gate before the file is placed, and the thumbnail made in [`commit`]
    /// decodes the body, so together they ensure every stored photo can be
    /// thumbnailed.
    fn check_header(&self, format: ImageFormat) -> Result<(), UploadError> {
        let mut reader = ImageReader::open(&self.temp).context("opening the upload's temp file")?;
        reader.set_format(format.codec());
        reader.limits(thumbnail::decode_limits());
        let reason = match reader.into_decoder() {
            Ok(decoder) if decoder.total_bytes() <= thumbnail::MAX_DECODE_ALLOC => return Ok(()),
            Ok(decoder) => format!("decoding needs {} bytes", decoder.total_bytes()),
            Err(err) => err.to_string(),
        };
        info!(sha256 = %self.sha256, %reason, "refusing an unreadable upload");
        Err(UploadError::Unreadable)
    }
}

/// The longest title kept, in characters.
const MAX_TITLE_CHARS: usize = 255;

/// A safe attachment title from the client's `raw` filename: only its last
/// path component (either separator), without control or format characters
/// and surrounding whitespace, and at most [`MAX_TITLE_CHARS`] long. Falls
/// back to `photo.<ext>` when nothing usable is left.
pub fn clean_title(raw: Option<&str>, ext: &str) -> String {
    let name: String = raw
        .and_then(|raw| raw.rsplit(['/', '\\']).next())
        .unwrap_or_default()
        .chars()
        .filter(|&c| !c.is_control() && !is_format_char(c))
        .collect();
    match name.trim() {
        "" | "." | ".." => format!("photo.{ext}"),
        trimmed => {
            let end = trimmed
                .char_indices()
                .nth(MAX_TITLE_CHARS)
                .map_or(trimmed.len(), |(at, _)| at);
            trimmed[..end].trim_end().to_owned()
        }
    }
}

/// Unicode general category Cf (format characters, as of Unicode 15.1). They
/// are invisible, and some, such as the bidi override U+202E, make a name
/// display differently from what it is.
fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{AD}'
            | '\u{600}'..='\u{605}'
            | '\u{61C}'
            | '\u{6DD}'
            | '\u{70F}'
            | '\u{890}'..='\u{891}'
            | '\u{8E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Barrier;
    use std::thread;

    use super::*;
    use crate::db::TestDb;
    use crate::schema::thumbnails;
    use crate::svc::fixtures::{
        SampleIds, attachment as attachment_row, ingest_batch, jpeg, png, png_header,
        png_header_rgba16, png_truncated_body, seed_sample,
    };

    /// A migrated, seeded database and an empty data dir with `originals/`.
    struct Harness {
        db: TestDb,
        data: tempfile::TempDir,
        ids: SampleIds,
    }

    impl Harness {
        fn new() -> Self {
            let db = TestDb::new();
            let ids = seed_sample(&mut db.pool.get().unwrap());
            let data = tempfile::tempdir().unwrap();
            fs::create_dir(data.path().join("originals")).unwrap();
            Self { db, data, ids }
        }

        fn originals(&self) -> PathBuf {
            self.data.path().join("originals")
        }

        /// The names in `originals/`, sorted.
        fn files(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(self.originals())
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            names
        }

        /// Streams `bytes` in small chunks, like a multipart body arrives.
        fn stage(&self, bytes: &[u8]) -> Staged {
            let mut upload = TempUpload::create(&self.originals()).unwrap();
            for chunk in bytes.chunks(7) {
                upload.write(chunk).unwrap();
            }
            upload.finish().unwrap()
        }

        fn upload(
            &self,
            entity_id: &str,
            bytes: &[u8],
            filename: Option<&str>,
            primary: bool,
        ) -> Result<StoredUpload, UploadError> {
            let staged = self.stage(bytes);
            store(
                &mut self.db.pool.get().unwrap(),
                self.data.path(),
                entity_id,
                staged,
                filename,
                primary,
            )
        }

        /// Ids of the photos of `entity_id` flagged primary.
        fn primaries(&self, entity_id: &str) -> Vec<String> {
            attachments::table
                .filter(attachments::entity_id.eq(entity_id))
                .filter(attachments::is_primary.eq(true))
                .select(attachments::id)
                .load(&mut self.db.pool.get().unwrap())
                .unwrap()
        }
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    #[test]
    fn stores_a_jpeg_and_makes_the_first_photo_primary() {
        let h = Harness::new();
        let bytes = jpeg(64, 48);

        let stored = h
            .upload(&h.ids.screws, &bytes, Some("IMG_0001.JPG"), false)
            .unwrap()
            .attachment;

        let sha = sha256_hex(&bytes);
        assert_eq!(stored.entity_id, h.ids.screws);
        assert_eq!(stored.kind, AttachmentKind::Photo);
        assert!(stored.is_primary, "the first photo becomes primary");
        assert_eq!(stored.title, "IMG_0001.JPG");
        assert_eq!(stored.mime_type, "image/jpeg");
        assert_eq!(stored.sha256, sha);
        assert_eq!(stored.size_bytes, i64::try_from(bytes.len()).unwrap());
        assert_eq!(h.files(), [sha.as_str()]);
        assert_eq!(
            fs::read(attachment::original_path(h.data.path(), &sha)).unwrap(),
            bytes
        );
        let mut conn = h.db.pool.get().unwrap();
        assert_eq!(
            attachment::get(&mut conn, &stored.id).unwrap(),
            Some(stored)
        );
    }

    #[test]
    fn the_mime_comes_from_the_bytes_not_the_filename() {
        let h = Harness::new();
        let stored = h
            .upload(&h.ids.screws, &png(8, 8), Some("holiday.jpg"), false)
            .unwrap()
            .attachment;
        assert_eq!(stored.mime_type, "image/png");
        assert_eq!(stored.title, "holiday.jpg");
        let unnamed = h
            .upload(&h.ids.screws, &png(9, 9), None, false)
            .unwrap()
            .attachment;
        assert_eq!(unnamed.title, "photo.png");
    }

    #[test]
    fn second_upload_with_primary_true_takes_over_and_exactly_one_is_primary() {
        // Review focus 5.
        let h = Harness::new();
        let first = h
            .upload(&h.ids.screws, &jpeg(10, 10), None, false)
            .unwrap()
            .attachment;
        let second = h
            .upload(&h.ids.screws, &jpeg(11, 11), None, false)
            .unwrap()
            .attachment;
        assert!(
            !second.is_primary,
            "a later photo does not take over by default"
        );
        assert_eq!(h.primaries(&h.ids.screws), [first.id.as_str()]);

        let third = h
            .upload(&h.ids.screws, &jpeg(12, 12), None, true)
            .unwrap()
            .attachment;

        assert!(third.is_primary);
        assert_eq!(h.primaries(&h.ids.screws), [third.id.as_str()]);
        let mut conn = h.db.pool.get().unwrap();
        let shown = attachment::primary_photo(&mut conn, &h.ids.screws)
            .unwrap()
            .unwrap();
        assert_eq!(shown.id, third.id);
        // Another entity's primary photo is untouched.
        assert_eq!(h.primaries(&h.ids.drill), [h.ids.photo.as_str()]);
    }

    #[test]
    fn identical_bytes_dedupe_to_one_file_with_two_rows_and_refcounted_delete() {
        // Review focus 2.
        let h = Harness::new();
        let bytes = jpeg(32, 32);
        let a = h
            .upload(&h.ids.screws, &bytes, None, false)
            .unwrap()
            .attachment;
        let b = h
            .upload(&h.ids.old_tv, &bytes, None, false)
            .unwrap()
            .attachment;
        assert_ne!(a.id, b.id);
        assert_eq!(a.sha256, b.sha256);
        assert_eq!(h.files(), [a.sha256.as_str()]);
        let mut conn = h.db.pool.get().unwrap();
        let thumbs = |conn: &mut SqliteConnection| -> i64 {
            thumbnails::table
                .filter(thumbnails::sha256.eq(&a.sha256))
                .count()
                .get_result(conn)
                .unwrap()
        };
        assert_eq!(thumbs(&mut conn), 1, "the two uploads share one thumbnail");

        attachment::delete(&mut conn, h.data.path(), &a.id).unwrap();
        assert_eq!(h.files(), [a.sha256.as_str()], "a shared original stays");
        assert_eq!(thumbs(&mut conn), 1, "a shared thumbnail stays");
        attachment::delete(&mut conn, h.data.path(), &b.id).unwrap();
        assert!(h.files().is_empty(), "the last delete removes the file");
        assert_eq!(
            thumbs(&mut conn),
            0,
            "the last delete removes the thumbnail"
        );
    }

    #[test]
    fn html_disguised_as_jpeg_is_unsupported_and_leaves_no_file() {
        // Review focus 1: the filename says JPEG, the bytes say HTML.
        let h = Harness::new();
        let html = b"<!doctype html><script>alert(1)</script>";

        let err = h
            .upload(&h.ids.screws, html, Some("photo.jpg"), false)
            .unwrap_err();

        assert!(matches!(err, UploadError::Unsupported), "{err:?}");
        assert!(h.files().is_empty(), "{:?}", h.files());
        assert!(h.primaries(&h.ids.screws).is_empty());
    }

    #[test]
    fn truncated_png_header_is_unreadable() {
        let h = Harness::new();
        let truncated = &png(16, 16)[..20];

        let err = h.upload(&h.ids.screws, truncated, None, false).unwrap_err();

        assert!(matches!(err, UploadError::Unreadable), "{err:?}");
        assert!(h.files().is_empty(), "{:?}", h.files());
    }

    #[test]
    fn a_truncated_body_behind_a_good_header_is_unreadable_and_leaves_nothing() {
        // The header decodes, so only decoding the pixels (the thumbnail
        // made at upload) finds the damage.
        let h = Harness::new();
        let err = h
            .upload(&h.ids.screws, &png_truncated_body(64, 64), None, false)
            .unwrap_err();
        assert!(matches!(err, UploadError::Unreadable), "{err:?}");
        assert!(h.files().is_empty(), "{:?}", h.files());
        let rows: i64 = attachments::table
            .filter(attachments::entity_id.eq(&h.ids.screws))
            .count()
            .get_result(&mut h.db.pool.get().unwrap())
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn an_unreadable_upload_sharing_an_imported_original_leaves_it_in_place() {
        // Imported originals are never decode-checked, so a damaged one can
        // already be stored when an upload brings the same bytes.
        let h = Harness::new();
        let bytes = png_truncated_body(64, 64);
        let sha = sha256_hex(&bytes);
        let original = attachment::original_path(h.data.path(), &sha);
        fs::write(&original, &bytes).unwrap();
        let imported = attachment_row(
            "imported-photo",
            &h.ids.old_tv,
            AttachmentKind::Photo,
            true,
            &sha,
            7,
            50,
        );
        let mut conn = h.db.pool.get().unwrap();
        diesel::insert_into(attachments::table)
            .values(&imported)
            .execute(&mut conn)
            .unwrap();
        drop(conn);

        let err = h.upload(&h.ids.screws, &bytes, None, false).unwrap_err();

        assert!(matches!(err, UploadError::Unreadable), "{err:?}");
        assert_eq!(h.files(), [sha.as_str()], "no temp file, original kept");
        assert_eq!(fs::read(&original).unwrap(), bytes);
        let mut conn = h.db.pool.get().unwrap();
        assert_eq!(
            attachment::get(&mut conn, "imported-photo").unwrap(),
            Some(imported)
        );
        assert!(h.primaries(&h.ids.screws).is_empty());
    }

    #[test]
    fn a_stored_upload_has_its_smallest_thumbnail_already() {
        let h = Harness::new();
        let stored = h
            .upload(&h.ids.screws, &jpeg(640, 480), None, false)
            .unwrap()
            .attachment;
        let thumb = attachment::thumbnail(&mut h.db.pool.get().unwrap(), &stored.sha256, 300)
            .unwrap()
            .expect("the 300px thumbnail is stored with the upload");
        assert_eq!((thumb.width, thumb.height), (300, 225));
        assert_eq!(thumb.mime_type, "image/webp");
    }

    #[test]
    fn dimensions_over_the_decoder_limits_are_unreadable() {
        // Uploads share the thumbnailer's limits, so every stored photo can be
        // thumbnailed.
        let h = Harness::new();
        let err = h
            .upload(&h.ids.screws, &png_header(20_000, 20_000), None, false)
            .unwrap_err();
        assert!(matches!(err, UploadError::Unreadable), "{err:?}");
        assert!(h.files().is_empty(), "{:?}", h.files());
    }

    #[test]
    fn a_decode_buffer_over_the_allocation_limit_is_unreadable() {
        // 8192² is within the edge limit, but 16-bit RGBA needs 512 MiB.
        let h = Harness::new();
        let err = h
            .upload(&h.ids.screws, &png_header_rgba16(8192, 8192), None, false)
            .unwrap_err();
        assert!(matches!(err, UploadError::Unreadable), "{err:?}");
        assert!(h.files().is_empty(), "{:?}", h.files());
    }

    #[test]
    fn unknown_entity_is_not_found() {
        let h = Harness::new();
        let err = h
            .upload("no-such-entity", &jpeg(8, 8), None, false)
            .unwrap_err();
        assert!(
            matches!(err, UploadError::NotFound(UploadTarget::Entity)),
            "{err:?}"
        );
        assert!(h.files().is_empty(), "{:?}", h.files());
    }

    #[test]
    fn a_failed_insert_removes_the_newly_placed_original() {
        let h = Harness::new();
        let mut conn = h.db.pool.get().unwrap();
        // The entity exists when checked but the row cannot be inserted.
        diesel::sql_query(
            "CREATE TRIGGER refuse_uploads BEFORE INSERT ON attachments \
             BEGIN SELECT RAISE(ABORT, 'refused'); END",
        )
        .execute(&mut conn)
        .unwrap();
        drop(conn);

        let err = h
            .upload(&h.ids.screws, &jpeg(8, 8), None, false)
            .unwrap_err();

        assert!(matches!(err, UploadError::Io(_)), "{err:?}");
        assert!(h.files().is_empty(), "{:?}", h.files());
    }

    #[test]
    fn a_placed_upload_keeps_its_original_through_a_delete_of_the_last_sharer() {
        // Carry-over (a): identical bytes are placed (deduped onto the
        // existing file) while the only other sharer is deleted.
        let h = Harness::new();
        let bytes = jpeg(24, 24);
        let first = h
            .upload(&h.ids.screws, &bytes, None, false)
            .unwrap()
            .attachment;
        let placed = place(h.data.path(), h.stage(&bytes)).unwrap();
        let mut conn = h.db.pool.get().unwrap();

        attachment::delete(&mut conn, h.data.path(), &first.id).unwrap();
        let stored = commit(&mut conn, h.data.path(), &h.ids.old_tv, placed, None, false)
            .unwrap()
            .attachment;

        assert_eq!(h.files(), [stored.sha256.as_str()]);
    }

    #[test]
    fn an_entity_deleted_after_placing_is_not_found_and_leaves_no_file() {
        let h = Harness::new();
        let placed = place(h.data.path(), h.stage(&jpeg(8, 8))).unwrap();
        let mut conn = h.db.pool.get().unwrap();

        let err = commit(
            &mut conn,
            h.data.path(),
            "deleted-meanwhile",
            placed,
            None,
            false,
        )
        .unwrap_err();

        assert!(
            matches!(err, UploadError::NotFound(UploadTarget::Entity)),
            "{err:?}"
        );
        assert!(h.files().is_empty(), "{:?}", h.files());
    }

    #[test]
    fn stages_an_ingest_photo_with_its_smallest_thumbnail() {
        let h = Harness::new();
        let (_, item) = ingest_batch(&mut h.db.pool.get().unwrap(), None);
        let bytes = jpeg(640, 480);
        let stage = |bytes: &[u8]| {
            store_ingest_photo(
                &mut h.db.pool.get().unwrap(),
                h.data.path(),
                &item.id,
                h.stage(bytes),
                Some("IMG_1.JPG"),
            )
        };

        let first = stage(&bytes).unwrap();
        let second = stage(&jpeg(8, 8)).unwrap();

        let sha = sha256_hex(&bytes);
        assert_eq!((first.position, second.position), (0, 1));
        assert_eq!(first.item_id, item.id);
        assert_eq!(first.sha256, sha);
        assert_eq!(first.title, "IMG_1.JPG");
        assert_eq!(first.mime_type, "image/jpeg");
        assert_eq!(first.size_bytes, i64::try_from(bytes.len()).unwrap());
        let mut conn = h.db.pool.get().unwrap();
        assert_eq!(
            ingest::get_photo(&mut conn, &first.id).unwrap(),
            Some(first)
        );
        let thumb = attachment::thumbnail(&mut conn, &sha, 300).unwrap();
        assert!(thumb.is_some(), "the 300px thumbnail is stored with it");
        assert_eq!(h.files().len(), 2);
    }

    #[test]
    fn an_unknown_ingest_item_is_not_found_and_a_submitted_batch_a_conflict() {
        let h = Harness::new();
        let mut conn = h.db.pool.get().unwrap();
        let err = store_ingest_photo(
            &mut conn,
            h.data.path(),
            "no-such-item",
            h.stage(&jpeg(8, 8)),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, UploadError::NotFound(UploadTarget::IngestItem)),
            "{err:?}"
        );
        let (batch, item) = ingest_batch(&mut conn, None);
        store_ingest_photo(
            &mut conn,
            h.data.path(),
            &item.id,
            h.stage(&jpeg(9, 9)),
            None,
        )
        .unwrap();
        ingest::submit(&mut conn, &batch.id).unwrap();

        let err = store_ingest_photo(
            &mut conn,
            h.data.path(),
            &item.id,
            h.stage(&jpeg(10, 10)),
            None,
        )
        .unwrap_err();

        assert!(matches!(err, UploadError::Conflict), "{err:?}");
        assert_eq!(h.files(), [sha256_hex(&jpeg(9, 9))]);
    }

    #[test]
    fn a_batch_submitted_after_placing_is_a_conflict_and_leaves_no_file() {
        // The early check passed, then the batch was submitted: the insert's
        // own check refuses, and the newly placed original goes.
        let h = Harness::new();
        let mut conn = h.db.pool.get().unwrap();
        let (batch, item) = ingest_batch(&mut conn, None);
        let kept = store_ingest_photo(
            &mut conn,
            h.data.path(),
            &item.id,
            h.stage(&jpeg(9, 9)),
            None,
        )
        .unwrap();
        let placed = place(h.data.path(), h.stage(&jpeg(8, 8))).unwrap();
        ingest::submit(&mut conn, &batch.id).unwrap();

        let err =
            commit_ingest_photo(&mut conn, h.data.path(), &item.id, placed, None).unwrap_err();

        assert!(matches!(err, UploadError::Conflict), "{err:?}");
        assert_eq!(h.files(), [kept.sha256.as_str()]);
    }

    #[test]
    fn simultaneous_first_uploads_all_succeed_with_one_primary() {
        let h = Harness::new();
        let uploads = 6;
        let staged: Vec<Staged> = (0..uploads).map(|n| h.stage(&jpeg(8 + n, 8))).collect();
        let start = Barrier::new(staged.len());
        let results: Vec<Result<StoredUpload, UploadError>> = thread::scope(|scope| {
            let workers: Vec<_> = staged
                .into_iter()
                .map(|staged| {
                    let (h, start) = (&h, &start);
                    scope.spawn(move || {
                        let mut conn = h.db.pool.get().unwrap();
                        start.wait();
                        store(&mut conn, h.data.path(), &h.ids.screws, staged, None, false)
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });

        for result in &results {
            assert!(result.is_ok(), "{result:?}");
        }
        assert_eq!(h.primaries(&h.ids.screws).len(), 1);
    }

    #[test]
    fn temp_file_is_removed_when_not_finished() {
        let h = Harness::new();
        let mut upload = TempUpload::create(&h.originals()).unwrap();
        upload.write(b"partial body").unwrap();
        let names = h.files();
        assert_eq!(names.len(), 1);
        assert!(names[0].starts_with(".upload-"), "{names:?}");

        drop(upload);
        assert!(h.files().is_empty(), "{:?}", h.files());

        let staged = h.stage(&jpeg(4, 4));
        assert_eq!(h.files().len(), 1);
        drop(staged);
        assert!(h.files().is_empty(), "{:?}", h.files());
    }

    #[test]
    fn finish_reports_the_hash_size_and_head() {
        let h = Harness::new();
        let bytes = jpeg(20, 20);
        let staged = h.stage(&bytes);
        assert_eq!(staged.sha256, sha256_hex(&bytes));
        assert_eq!(staged.size_bytes, u64::try_from(bytes.len()).unwrap());
        assert_eq!(staged.head, bytes[..SNIFF_LEN]);

        // A body shorter than the head is zero-padded, never read past.
        let short = h.stage(b"GIF89a");
        assert_eq!(short.size_bytes, 6);
        assert_eq!(short.head[..6], *b"GIF89a");
    }

    #[test]
    fn clean_title_strips_paths_and_controls() {
        let cases = [
            (Some("IMG_0001.JPG"), "IMG_0001.JPG"),
            (Some("C:\\Users\\me\\Pictures\\drill.jpg"), "drill.jpg"),
            (Some("../../etc/passwd"), "passwd"),
            (Some("a\r\nb\t.png"), "ab.png"),
            (Some("  spaced name.jpg  "), "spaced name.jpg"),
            (Some("dir/"), "photo.jpg"),
            (Some(".."), "photo.jpg"),
            (Some("."), "photo.jpg"),
            (Some("\u{7}\u{1b}"), "photo.jpg"),
            (Some(""), "photo.jpg"),
            (None, "photo.jpg"),
            // A right-to-left override would display "evil\u{202E}gpj.exe" as
            // "evilexe.jpg"; zero-width and BOM characters are invisible.
            (Some("evil\u{202E}gpj.exe"), "evilgpj.exe"),
            (Some("\u{FEFF}a\u{200B}b.jpg"), "ab.jpg"),
            (Some("\u{202E}\u{200D}"), "photo.jpg"),
        ];
        for (raw, expected) in cases {
            assert_eq!(clean_title(raw, "jpg"), expected, "{raw:?}");
        }
    }

    #[test]
    fn clean_title_is_capped_at_255_characters() {
        let long = format!("{}.jpg", "é".repeat(300));
        let title = clean_title(Some(&long), "jpg");
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert!(title.chars().all(|c| c == 'é'));
        // Trailing whitespace exposed by the cut is trimmed too.
        let spaced = format!("{} {}", "a".repeat(254), "b".repeat(10));
        assert_eq!(clean_title(Some(&spaced), "jpg"), "a".repeat(254));
    }
}
