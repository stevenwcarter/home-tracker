//! AI ingest staging: batches of items, each a group of photos that the
//! runner describes and synthesises into a suggestion, which the user then
//! accepts as an entity or skips. Staged photos live in the same originals
//! store as attachments and are counted as sharers of their sha256.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use anyhow::{Context, Result, anyhow, ensure};
use chrono::{NaiveDateTime, Utc};
use diesel::dsl::max;
use diesel::prelude::*;
use serde_json::Value;
use uuid::Uuid;

use crate::kinds::{
    AttachmentKind, IngestBatchStatus, IngestItemStatus, IngestPhotoStatus, SuggestedKind,
};
use crate::models::{Attachment, Entity, IngestBatch, IngestItem, IngestPhoto};
use crate::schema::{attachments, ingest_batches, ingest_items, ingest_photos};
use crate::svc::entity::{self, EntityInput};
use crate::svc::{attachment, thumbnail};

/// Which ingest row a [`IngestError::NotFound`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestRecord {
    Batch,
    Item,
    Photo,
}

impl fmt::Display for IngestRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Batch => "ingest batch",
            Self::Item => "ingest item",
            Self::Photo => "ingest photo",
        })
    }
}

/// Why an ingest operation was refused. Returned inside `anyhow::Error`;
/// callers that map it to a status `downcast_ref::<IngestError>()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IngestError {
    #[error("{0} not found")]
    NotFound(IngestRecord),
    #[error("this batch has been submitted and can no longer change")]
    NotCollecting,
    #[error("only an item that is ready or failed can be accepted or skipped")]
    NotReviewable,
    #[error("only an item that is ready or failed can be retried")]
    NotRetryable,
    #[error("add at least one photo before submitting")]
    Empty,
    #[error("photo kinds name a photo of another item")]
    ForeignPhoto,
}

/// The ingest batch with `id`, if any.
pub fn get_batch(conn: &mut SqliteConnection, id: &str) -> Result<Option<IngestBatch>> {
    ingest_batches::table
        .find(id)
        .select(IngestBatch::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading ingest batch {id:?}"))
}

/// The ingest item with `id`, if any.
pub fn get_item(conn: &mut SqliteConnection, id: &str) -> Result<Option<IngestItem>> {
    ingest_items::table
        .find(id)
        .select(IngestItem::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading ingest item {id:?}"))
}

/// The staged photo with `id`, if any.
pub fn get_photo(conn: &mut SqliteConnection, id: &str) -> Result<Option<IngestPhoto>> {
    ingest_photos::table
        .find(id)
        .select(IngestPhoto::as_select())
        .first(conn)
        .optional()
        .with_context(|| format!("loading ingest photo {id:?}"))
}

/// The batch item `item_id` belongs to.
pub fn batch_of(conn: &mut SqliteConnection, item_id: &str) -> Result<IngestBatch> {
    let item = require_item(conn, item_id)?;
    require_batch(conn, &item.batch_id)
}

/// The items of batch `batch_id` in position order.
pub fn items(conn: &mut SqliteConnection, batch_id: &str) -> Result<Vec<IngestItem>> {
    ingest_items::table
        .filter(ingest_items::batch_id.eq(batch_id))
        .order(ingest_items::position.asc())
        .select(IngestItem::as_select())
        .load(conn)
        .with_context(|| format!("loading the items of ingest batch {batch_id:?}"))
}

/// The staged photos of item `item_id` in position order.
pub fn photos(conn: &mut SqliteConnection, item_id: &str) -> Result<Vec<IngestPhoto>> {
    ingest_photos::table
        .filter(ingest_photos::item_id.eq(item_id))
        .order(ingest_photos::position.asc())
        .select(IngestPhoto::as_select())
        .load(conn)
        .with_context(|| format!("loading the photos of ingest item {item_id:?}"))
}

/// The batches started from `parent_id` (`None`: from no entity) that are
/// not done yet, newest first: what a page offers to resume.
pub fn open_batches(
    conn: &mut SqliteConnection,
    parent_id: Option<&str>,
) -> Result<Vec<IngestBatch>> {
    let open = ingest_batches::table
        .filter(ingest_batches::status.ne(IngestBatchStatus::Done))
        .order((ingest_batches::created_at.desc(), ingest_batches::id.desc()))
        .select(IngestBatch::as_select())
        .into_boxed();
    match parent_id {
        Some(parent_id) => open.filter(ingest_batches::parent_id.eq(parent_id)),
        None => open.filter(ingest_batches::parent_id.is_null()),
    }
    .load(conn)
    .with_context(|| format!("loading the open ingest batches of {parent_id:?}"))
}

/// Starts a collecting batch under `parent_id`, which must exist, with one
/// empty item for the first photos to go into.
pub fn create_batch(conn: &mut SqliteConnection, parent_id: Option<&str>) -> Result<IngestBatch> {
    conn.transaction(|conn| {
        if let Some(parent_id) = parent_id {
            entity::get(conn, parent_id)?.ok_or_else(|| anyhow!("parent not found"))?;
        }
        let now = now();
        let batch = IngestBatch {
            id: Uuid::now_v7().to_string(),
            parent_id: parent_id.map(str::to_owned),
            status: IngestBatchStatus::Collecting,
            created_at: now,
            updated_at: now,
        };
        diesel::insert_into(ingest_batches::table)
            .values(&batch)
            .execute(conn)
            .context("creating an ingest batch")?;
        insert_item(conn, &batch.id, 0)?;
        Ok(batch)
    })
}

/// Appends an empty item to batch `batch_id`, which must still be collecting.
pub fn add_item(conn: &mut SqliteConnection, batch_id: &str) -> Result<IngestItem> {
    conn.transaction(|conn| {
        require_collecting(conn, batch_id)?;
        let last: Option<i32> = ingest_items::table
            .filter(ingest_items::batch_id.eq(batch_id))
            .select(max(ingest_items::position))
            .first(conn)
            .with_context(|| format!("finding the last item of ingest batch {batch_id:?}"))?;
        let item = insert_item(conn, batch_id, last.map_or(0, |p| p + 1))?;
        touch(conn, batch_id)?;
        Ok(item)
    })
}

/// Inserts a collecting item of `batch_id` at `position`.
fn insert_item(conn: &mut SqliteConnection, batch_id: &str, position: i32) -> Result<IngestItem> {
    let now = now();
    let item = IngestItem {
        id: Uuid::now_v7().to_string(),
        batch_id: batch_id.to_owned(),
        position,
        status: IngestItemStatus::Collecting,
        error: None,
        suggestion: None,
        entity_id: None,
        created_at: now,
        updated_at: now,
    };
    diesel::insert_into(ingest_items::table)
        .values(&item)
        .execute(conn)
        .with_context(|| format!("adding an item to ingest batch {batch_id:?}"))?;
    Ok(item)
}

/// Removes item `id` and its photos while its batch is collecting. Originals
/// nothing else shares are removed from `data_dir` after the delete commits.
pub fn remove_item(conn: &mut SqliteConnection, data_dir: &Path, id: &str) -> Result<()> {
    let shas = conn.transaction(|conn| {
        let item = require_item(conn, id)?;
        require_collecting(conn, &item.batch_id)?;
        let shas = delete_photos_of(conn, &[id])?;
        diesel::delete(ingest_items::table.find(id))
            .execute(conn)
            .with_context(|| format!("deleting ingest item {id:?}"))?;
        touch(conn, &item.batch_id)?;
        Ok::<_, anyhow::Error>(shas)
    })?;
    remove_originals(conn, data_dir, &shas);
    Ok(())
}

/// Removes staged photo `id` while its batch is collecting; its original goes
/// too once nothing else shares it.
pub fn remove_photo(conn: &mut SqliteConnection, data_dir: &Path, id: &str) -> Result<()> {
    let sha256 = conn.transaction(|conn| {
        let photo = require_photo(conn, id)?;
        let item = require_item(conn, &photo.item_id)?;
        require_collecting(conn, &item.batch_id)?;
        diesel::delete(ingest_photos::table.find(id))
            .execute(conn)
            .with_context(|| format!("deleting ingest photo {id:?}"))?;
        touch(conn, &item.batch_id)?;
        Ok::<_, anyhow::Error>(photo.sha256)
    })?;
    attachment::remove_original(conn, data_dir, &sha256);
    Ok(())
}

/// Records a pending photo of item `item_id` whose bytes are already stored
/// under `sha256`, after the item's other photos. Refused with
/// [`IngestError::NotCollecting`] once the batch is submitted.
pub fn insert_photo_row(
    conn: &mut SqliteConnection,
    item_id: &str,
    sha256: &str,
    mime_type: &str,
    size_bytes: i64,
    title: &str,
) -> Result<IngestPhoto> {
    conn.transaction(|conn| {
        let item = require_item(conn, item_id)?;
        require_collecting(conn, &item.batch_id)?;
        let last: Option<i32> = ingest_photos::table
            .filter(ingest_photos::item_id.eq(item_id))
            .select(max(ingest_photos::position))
            .first(conn)
            .with_context(|| format!("finding the last photo of ingest item {item_id:?}"))?;
        let photo = IngestPhoto {
            id: Uuid::now_v7().to_string(),
            item_id: item_id.to_owned(),
            position: last.map_or(0, |p| p + 1),
            sha256: sha256.to_owned(),
            mime_type: mime_type.to_owned(),
            size_bytes,
            title: title.to_owned(),
            status: IngestPhotoStatus::Pending,
            error: None,
            description: None,
            suggested_kind: None,
            created_at: now(),
        };
        diesel::insert_into(ingest_photos::table)
            .values(&photo)
            .execute(conn)
            .with_context(|| format!("staging a photo for ingest item {item_id:?}"))?;
        touch(conn, &item.batch_id)?;
        Ok(photo)
    })
}

/// Hands collecting batch `batch_id` to the runner: items without photos are
/// dropped, the rest are queued, and the batch starts processing. A batch
/// with no photo at all is refused with [`IngestError::Empty`], unchanged.
pub fn submit(conn: &mut SqliteConnection, batch_id: &str) -> Result<IngestBatch> {
    conn.transaction(|conn| {
        require_collecting(conn, batch_id)?;
        let in_batch = ingest_items::batch_id.eq(batch_id);
        let with_photos = ingest_photos::table.select(ingest_photos::item_id);
        diesel::delete(
            ingest_items::table
                .filter(in_batch)
                .filter(ingest_items::id.ne_all(with_photos)),
        )
        .execute(conn)
        .with_context(|| format!("dropping the empty items of ingest batch {batch_id:?}"))?;
        let queued = diesel::update(ingest_items::table.filter(in_batch))
            .set((
                ingest_items::status.eq(IngestItemStatus::Queued),
                ingest_items::updated_at.eq(now()),
            ))
            .execute(conn)
            .with_context(|| format!("queueing the items of ingest batch {batch_id:?}"))?;
        ensure!(queued > 0, IngestError::Empty);
        set_batch_status(conn, batch_id, IngestBatchStatus::Processing)
    })
}

/// Stores the vision step's `description` of photo `id` and the kind it
/// suggests.
pub fn mark_photo_described(
    conn: &mut SqliteConnection,
    id: &str,
    description: &Value,
    kind: SuggestedKind,
) -> Result<()> {
    let updated = diesel::update(ingest_photos::table.find(id))
        .set((
            ingest_photos::status.eq(IngestPhotoStatus::Described),
            ingest_photos::error.eq(None::<String>),
            ingest_photos::description.eq(description.to_string()),
            ingest_photos::suggested_kind.eq(kind),
        ))
        .execute(conn)
        .with_context(|| format!("storing the description of ingest photo {id:?}"))?;
    found(updated, IngestRecord::Photo)
}

/// Marks photo `id` failed with a short, user-readable `error`.
pub fn mark_photo_failed(conn: &mut SqliteConnection, id: &str, error: &str) -> Result<()> {
    let updated = diesel::update(ingest_photos::table.find(id))
        .set((
            ingest_photos::status.eq(IngestPhotoStatus::Failed),
            ingest_photos::error.eq(error),
        ))
        .execute(conn)
        .with_context(|| format!("marking ingest photo {id:?} failed"))?;
    found(updated, IngestRecord::Photo)
}

/// Moves item `id` to `status` with `error` (cleared when `None`). Runner
/// progress counts as activity on the batch.
pub fn set_item_status(
    conn: &mut SqliteConnection,
    id: &str,
    status: IngestItemStatus,
    error: Option<&str>,
) -> Result<()> {
    conn.transaction(|conn| {
        let updated = diesel::update(ingest_items::table.find(id))
            .set((
                ingest_items::status.eq(status),
                ingest_items::error.eq(error),
                ingest_items::updated_at.eq(now()),
            ))
            .execute(conn)
            .with_context(|| format!("setting the status of ingest item {id:?}"))?;
        found(updated, IngestRecord::Item)?;
        touch_item_batch(conn, id)
    })
}

/// Stores the synthesis step's `suggestion` for item `id`.
pub fn set_item_suggestion(
    conn: &mut SqliteConnection,
    id: &str,
    suggestion: &Value,
) -> Result<()> {
    let updated = diesel::update(ingest_items::table.find(id))
        .set((
            ingest_items::suggestion.eq(suggestion.to_string()),
            ingest_items::updated_at.eq(now()),
        ))
        .execute(conn)
        .with_context(|| format!("storing the suggestion of ingest item {id:?}"))?;
    found(updated, IngestRecord::Item)
}

/// Puts ready or failed item `item_id` back in the queue, error cleared, for
/// the runner to analyse again; refused with [`IngestError::NotRetryable`]
/// otherwise. A reviewing batch goes back to processing, so a restart
/// mid-retry re-queues the item like any other interrupted one, and the
/// runner's settle returns the batch to reviewing.
pub fn retry(conn: &mut SqliteConnection, item_id: &str) -> Result<IngestItem> {
    // Immediate, as the runner may be writing to the same batch.
    conn.immediate_transaction(|conn| {
        let item = require_item(conn, item_id)?;
        ensure!(item.status.is_reviewable(), IngestError::NotRetryable);
        set_item_status(conn, item_id, IngestItemStatus::Queued, None)?;
        if require_batch(conn, &item.batch_id)?.status == IngestBatchStatus::Reviewing {
            set_batch_status(conn, &item.batch_id, IngestBatchStatus::Processing)?;
        }
        require_item(conn, item_id)
    })
}

/// Turns reviewable item `item_id` into an entity built from `input` (the
/// user's values, not the suggestion). Each staged photo becomes an
/// attachment of the kind `kinds` gives it, else its suggested kind, else
/// `photo`; the first `photo`-kind one that can be thumbnailed is the
/// primary photo (none if there is no such one). The staged rows
/// are deleted in the same transaction the attachments are inserted in, so
/// the originals are shared throughout and no file is touched.
pub fn accept(
    conn: &mut SqliteConnection,
    item_id: &str,
    input: EntityInput,
    kinds: &[(String, AttachmentKind)],
) -> Result<Entity> {
    conn.immediate_transaction(|conn| {
        let item = require_reviewable(conn, item_id)?;
        let staged = photos(conn, item_id)?;
        ensure!(
            kinds
                .iter()
                .all(|(id, _)| staged.iter().any(|photo| &photo.id == id)),
            IngestError::ForeignPhoto
        );
        let created = entity::create(conn, input)?;
        let chosen: Vec<AttachmentKind> = staged
            .iter()
            .map(|photo| {
                kinds
                    .iter()
                    .find_map(|(id, kind)| (*id == photo.id).then_some(*kind))
                    .or_else(|| photo.suggested_kind.map(AttachmentKind::from))
                    .unwrap_or(AttachmentKind::Photo)
            })
            .collect();
        // Like `promote_earliest_photo`: a photo nothing can thumbnail
        // would be a primary photo nothing can show.
        let primary = staged.iter().zip(&chosen).position(|(photo, &kind)| {
            kind == AttachmentKind::Photo && thumbnail::is_thumbnailable(&photo.mime_type)
        });
        let now = now();
        let rows: Vec<Attachment> = staged
            .into_iter()
            .zip(chosen)
            .enumerate()
            .map(|(index, (photo, kind))| Attachment {
                id: Uuid::now_v7().to_string(),
                entity_id: created.id.clone(),
                kind,
                is_primary: primary == Some(index),
                title: photo.title,
                mime_type: photo.mime_type,
                sha256: photo.sha256,
                size_bytes: photo.size_bytes,
                created_at: now,
                updated_at: now,
            })
            .collect();
        diesel::insert_into(attachments::table)
            .values(&rows)
            .execute(conn)
            .with_context(|| format!("attaching the photos of ingest item {item_id:?}"))?;
        diesel::delete(ingest_photos::table.filter(ingest_photos::item_id.eq(item_id)))
            .execute(conn)
            .with_context(|| format!("deleting the staged photos of ingest item {item_id:?}"))?;
        diesel::update(ingest_items::table.find(item_id))
            .set((
                ingest_items::status.eq(IngestItemStatus::Accepted),
                ingest_items::entity_id.eq(&created.id),
                ingest_items::updated_at.eq(now),
            ))
            .execute(conn)
            .with_context(|| format!("marking ingest item {item_id:?} accepted"))?;
        touch(conn, &item.batch_id)?;
        settle_batch(conn, &item.batch_id)?;
        Ok(created)
    })
}

/// Sets reviewable item `item_id` aside without creating anything: its
/// staged photos are deleted, and their originals once nothing else shares
/// them.
pub fn skip(conn: &mut SqliteConnection, data_dir: &Path, item_id: &str) -> Result<IngestItem> {
    // Immediate, as the runner may be writing to the same batch.
    let (item, shas) = conn.immediate_transaction(|conn| {
        let item = require_reviewable(conn, item_id)?;
        let shas = delete_photos_of(conn, &[item_id])?;
        diesel::update(ingest_items::table.find(item_id))
            .set((
                ingest_items::status.eq(IngestItemStatus::Skipped),
                ingest_items::updated_at.eq(now()),
            ))
            .execute(conn)
            .with_context(|| format!("marking ingest item {item_id:?} skipped"))?;
        touch(conn, &item.batch_id)?;
        settle_batch(conn, &item.batch_id)?;
        Ok::<_, anyhow::Error>((require_item(conn, item_id)?, shas))
    })?;
    remove_originals(conn, data_dir, &shas);
    Ok(item)
}

/// Deletes batch `id` with its items and staged photos, then the originals
/// nothing else shares.
pub fn delete_batch(conn: &mut SqliteConnection, data_dir: &Path, id: &str) -> Result<()> {
    let shas = conn.transaction(|conn| {
        require_batch(conn, id)?;
        delete_batches(conn, &[id])
    })?;
    remove_originals(conn, data_dir, &shas);
    Ok(())
}

/// Advances batch `batch_id` once its items allow: a processing batch whose
/// items are all ready, failed or closed goes to reviewing, and a processing
/// or reviewing batch whose items are all accepted or skipped is done. A
/// collecting or done batch is left alone. Returns the batch as it now is.
pub fn settle_batch(conn: &mut SqliteConnection, batch_id: &str) -> Result<IngestBatch> {
    let batch = require_batch(conn, batch_id)?;
    let statuses: Vec<IngestItemStatus> = ingest_items::table
        .filter(ingest_items::batch_id.eq(batch_id))
        .select(ingest_items::status)
        .load(conn)
        .with_context(|| format!("loading the item statuses of ingest batch {batch_id:?}"))?;
    let closed = statuses.iter().all(|s| s.is_closed());
    let settled = statuses.iter().all(|s| s.is_closed() || s.is_reviewable());
    let next = match batch.status {
        IngestBatchStatus::Processing | IngestBatchStatus::Reviewing if closed => {
            IngestBatchStatus::Done
        }
        IngestBatchStatus::Processing if settled => IngestBatchStatus::Reviewing,
        _ => return Ok(batch),
    };
    set_batch_status(conn, batch_id, next)
}

/// Deletes every batch with no activity since `cutoff`, whatever its status,
/// and the originals of its photos that nothing else shares. Returns how
/// many batches went.
pub fn cleanup_stale(
    conn: &mut SqliteConnection,
    data_dir: &Path,
    cutoff: NaiveDateTime,
) -> Result<usize> {
    let (count, shas) = conn.transaction(|conn| {
        let stale: Vec<String> = ingest_batches::table
            .filter(ingest_batches::updated_at.lt(cutoff))
            .select(ingest_batches::id)
            .load(conn)
            .context("finding stale ingest batches")?;
        let ids: Vec<&str> = stale.iter().map(String::as_str).collect();
        Ok::<_, anyhow::Error>((stale.len(), delete_batches(conn, &ids)?))
    })?;
    remove_originals(conn, data_dir, &shas);
    Ok(count)
}

/// After a restart: puts every item left `analysing` in a processing batch
/// back to `queued`, and returns the ids of all processing batches, whose
/// runs the restart cut short (including ones whose items were all still
/// queued).
pub fn requeue_interrupted(conn: &mut SqliteConnection) -> Result<Vec<String>> {
    conn.transaction(|conn| {
        let processing: Vec<String> = ingest_batches::table
            .filter(ingest_batches::status.eq(IngestBatchStatus::Processing))
            .order(ingest_batches::created_at.asc())
            .select(ingest_batches::id)
            .load(conn)
            .context("finding processing ingest batches")?;
        diesel::update(
            ingest_items::table
                .filter(ingest_items::batch_id.eq_any(&processing))
                .filter(ingest_items::status.eq(IngestItemStatus::Analysing)),
        )
        .set((
            ingest_items::status.eq(IngestItemStatus::Queued),
            ingest_items::updated_at.eq(now()),
        ))
        .execute(conn)
        .context("re-queueing interrupted ingest items")?;
        Ok(processing)
    })
}

/// Records activity on batch `batch_id`, postponing its cleanup.
pub fn touch(conn: &mut SqliteConnection, batch_id: &str) -> Result<()> {
    let updated = diesel::update(ingest_batches::table.find(batch_id))
        .set(ingest_batches::updated_at.eq(now()))
        .execute(conn)
        .with_context(|| format!("touching ingest batch {batch_id:?}"))?;
    found(updated, IngestRecord::Batch)
}

/// Records activity on the batch of item `item_id`.
fn touch_item_batch(conn: &mut SqliteConnection, item_id: &str) -> Result<()> {
    let batch_id = ingest_items::table
        .find(item_id)
        .select(ingest_items::batch_id);
    diesel::update(ingest_batches::table.filter(ingest_batches::id.eq_any(batch_id)))
        .set(ingest_batches::updated_at.eq(now()))
        .execute(conn)
        .with_context(|| format!("touching the batch of ingest item {item_id:?}"))?;
    Ok(())
}

fn set_batch_status(
    conn: &mut SqliteConnection,
    batch_id: &str,
    status: IngestBatchStatus,
) -> Result<IngestBatch> {
    diesel::update(ingest_batches::table.find(batch_id))
        .set((
            ingest_batches::status.eq(status),
            ingest_batches::updated_at.eq(now()),
        ))
        .returning(IngestBatch::as_returning())
        .get_result(conn)
        .optional()
        .with_context(|| format!("setting the status of ingest batch {batch_id:?}"))?
        .ok_or_else(|| IngestError::NotFound(IngestRecord::Batch).into())
}

/// Deletes batches `ids` (their items and photos cascade) and returns the
/// hashes their photos staged, for [`remove_originals`] after commit.
fn delete_batches(conn: &mut SqliteConnection, ids: &[&str]) -> Result<BTreeSet<String>> {
    let items: Vec<String> = ingest_items::table
        .filter(ingest_items::batch_id.eq_any(ids))
        .select(ingest_items::id)
        .load(conn)
        .context("finding the items of ingest batches")?;
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    let shas = delete_photos_of(conn, &items)?;
    diesel::delete(ingest_batches::table.filter(ingest_batches::id.eq_any(ids)))
        .execute(conn)
        .context("deleting ingest batches")?;
    Ok(shas)
}

/// Deletes the staged photos of items `item_ids` and returns the hashes they
/// staged, for [`remove_originals`] after commit.
fn delete_photos_of(conn: &mut SqliteConnection, item_ids: &[&str]) -> Result<BTreeSet<String>> {
    let shas: BTreeSet<String> = ingest_photos::table
        .filter(ingest_photos::item_id.eq_any(item_ids))
        .select(ingest_photos::sha256)
        .load::<String>(conn)
        .context("finding the hashes of staged photos")?
        .into_iter()
        .collect();
    diesel::delete(ingest_photos::table.filter(ingest_photos::item_id.eq_any(item_ids)))
        .execute(conn)
        .context("deleting staged photos")?;
    Ok(shas)
}

/// Removes each original of `shas` that nothing shares any more. Called
/// after the deleting transaction commits, so a rollback never strands a
/// row without its file.
fn remove_originals(conn: &mut SqliteConnection, data_dir: &Path, shas: &BTreeSet<String>) {
    for sha256 in shas {
        attachment::remove_original(conn, data_dir, sha256);
    }
}

fn now() -> NaiveDateTime {
    Utc::now().naive_utc()
}

/// `Ok` when an update touched a row, else [`IngestError::NotFound`].
fn found(updated: usize, record: IngestRecord) -> Result<()> {
    ensure!(updated > 0, IngestError::NotFound(record));
    Ok(())
}

fn require_batch(conn: &mut SqliteConnection, id: &str) -> Result<IngestBatch> {
    get_batch(conn, id)?.ok_or_else(|| IngestError::NotFound(IngestRecord::Batch).into())
}

fn require_item(conn: &mut SqliteConnection, id: &str) -> Result<IngestItem> {
    get_item(conn, id)?.ok_or_else(|| IngestError::NotFound(IngestRecord::Item).into())
}

fn require_photo(conn: &mut SqliteConnection, id: &str) -> Result<IngestPhoto> {
    get_photo(conn, id)?.ok_or_else(|| IngestError::NotFound(IngestRecord::Photo).into())
}

/// Batch `id`, refused with [`IngestError::NotCollecting`] once submitted.
pub(crate) fn require_collecting(conn: &mut SqliteConnection, id: &str) -> Result<IngestBatch> {
    let batch = require_batch(conn, id)?;
    ensure!(
        batch.status == IngestBatchStatus::Collecting,
        IngestError::NotCollecting
    );
    Ok(batch)
}

/// Item `id`, refused with [`IngestError::NotReviewable`] unless it is ready
/// or failed.
fn require_reviewable(conn: &mut SqliteConnection, id: &str) -> Result<IngestItem> {
    let item = require_item(conn, id)?;
    ensure!(item.status.is_reviewable(), IngestError::NotReviewable);
    Ok(item)
}

/// Where staged `photo`'s original is served, with the same `?v=` tag as
/// [`attachment::original_url`].
pub fn original_url(photo: &IngestPhoto) -> String {
    format!(
        "/ingest/photos/{}?v={}",
        photo.id,
        attachment::version_tag(&photo.sha256)
    )
}

/// Where a thumbnail of at most `size` pixels of staged `photo` is served;
/// `None` for non-images, as [`attachment::thumbnail_url`].
pub fn thumbnail_url(photo: &IngestPhoto, size: i32) -> Option<String> {
    thumbnail::is_thumbnailable(&photo.mime_type).then(|| {
        format!(
            "/ingest/photos/{}/thumb/{size}?v={}",
            photo.id,
            attachment::version_tag(&photo.sha256)
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use chrono::{TimeDelta, Utc};
    use serde_json::json;

    use super::*;
    use crate::db::{ITEM_TYPE_ID, TestDb};
    use crate::kinds::{IngestBatchStatus, IngestPhotoStatus};
    use crate::models::Thumbnail;
    use crate::schema::{entities, ingest_batches, ingest_items, ingest_photos};
    use crate::svc::attachment::{self, original_path};
    use crate::svc::fixtures::{at, entity_input, ingest_batch, ingest_photo, seed_sample};

    /// The [`IngestError`] inside `result`'s error.
    fn refusal<T: fmt::Debug>(result: Result<T>) -> IngestError {
        *result
            .unwrap_err()
            .downcast_ref::<IngestError>()
            .expect("an IngestError")
    }

    /// A data dir holding an original for each of `shas`.
    fn data_with(shas: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(attachment::originals_dir(data.path())).unwrap();
        let files = shas
            .iter()
            .map(|sha| {
                let file = original_path(data.path(), sha);
                fs::write(&file, sha.as_bytes()).unwrap();
                file
            })
            .collect();
        (data, files)
    }

    fn statuses(conn: &mut SqliteConnection, batch_id: &str) -> Vec<IngestItemStatus> {
        items(conn, batch_id)
            .unwrap()
            .into_iter()
            .map(|item| item.status)
            .collect()
    }

    fn batch_status(conn: &mut SqliteConnection, batch_id: &str) -> IngestBatchStatus {
        get_batch(conn, batch_id).unwrap().unwrap().status
    }

    /// A submitted batch under `parent` with one item holding a JPEG per
    /// entry of `shas`, marked ready for review.
    fn ready_item(
        conn: &mut SqliteConnection,
        parent: Option<&str>,
        shas: &[&str],
    ) -> (IngestBatch, IngestItem, Vec<IngestPhoto>) {
        let (batch, item) = ingest_batch(conn, parent);
        let photos = shas
            .iter()
            .map(|sha| ingest_photo(conn, &item.id, sha, "image/jpeg"))
            .collect();
        submit(conn, &batch.id).unwrap();
        set_item_status(conn, &item.id, IngestItemStatus::Ready, None).unwrap();
        (batch, item, photos)
    }

    /// Inserts a ready item with one staged photo straight into submitted
    /// batch `batch_id` (submission forbids `add_item`) and returns its id.
    fn ready_item_in(conn: &mut SqliteConnection, batch_id: &str) -> String {
        let position: i32 = ingest_items::table
            .filter(ingest_items::batch_id.eq(batch_id))
            .count()
            .get_result::<i64>(conn)
            .unwrap()
            .try_into()
            .unwrap();
        let item = insert_item(conn, batch_id, position + 10).unwrap();
        set_item_status(conn, &item.id, IngestItemStatus::Ready, None).unwrap();
        stage_row(conn, &item.id, &"ab".repeat(32), "image/jpeg", None);
        item.id
    }

    /// A ready item of a new submitted batch whose photos are the rows
    /// `(sha256, mime_type, suggested kind)`, inserted directly so a test can
    /// stage a MIME type the upload would refuse. Returns the item id.
    fn staged_item(
        conn: &mut SqliteConnection,
        rows: &[(&str, &str, Option<SuggestedKind>)],
    ) -> String {
        let (batch, item) = ingest_batch(conn, None);
        for &(sha256, mime_type, kind) in rows {
            stage_row(conn, &item.id, sha256, mime_type, kind);
        }
        submit(conn, &batch.id).unwrap();
        set_item_status(conn, &item.id, IngestItemStatus::Ready, None).unwrap();
        item.id
    }

    /// Inserts a staged photo row directly, bypassing the collecting check,
    /// with `kind` as its suggested kind.
    fn stage_row(
        conn: &mut SqliteConnection,
        item_id: &str,
        sha256: &str,
        mime_type: &str,
        kind: Option<SuggestedKind>,
    ) -> IngestPhoto {
        let position: i32 = ingest_photos::table
            .filter(ingest_photos::item_id.eq(item_id))
            .count()
            .get_result::<i64>(conn)
            .unwrap()
            .try_into()
            .unwrap();
        let photo = IngestPhoto {
            id: Uuid::now_v7().to_string(),
            item_id: item_id.to_owned(),
            position,
            sha256: sha256.to_owned(),
            mime_type: mime_type.to_owned(),
            size_bytes: 3,
            title: "photo".to_owned(),
            status: IngestPhotoStatus::Described,
            error: None,
            description: None,
            suggested_kind: kind,
            created_at: now(),
        };
        diesel::insert_into(ingest_photos::table)
            .values(&photo)
            .execute(conn)
            .unwrap();
        photo
    }

    #[test]
    fn create_batch_needs_an_existing_parent_and_starts_with_one_item() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);

        let err = create_batch(&mut conn, Some("missing")).unwrap_err();
        assert_eq!(err.to_string(), "parent not found");
        let batches: i64 = ingest_batches::table.count().get_result(&mut conn).unwrap();
        assert_eq!(batches, 0);

        let batch = create_batch(&mut conn, Some(&ids.garage)).unwrap();
        assert_eq!(batch.status, IngestBatchStatus::Collecting);
        assert_eq!(batch.parent_id.as_deref(), Some(ids.garage.as_str()));
        assert_eq!(
            get_batch(&mut conn, &batch.id).unwrap(),
            Some(batch.clone())
        );
        let listed = items(&mut conn, &batch.id).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].position, 0);
        assert_eq!(listed[0].status, IngestItemStatus::Collecting);
        assert!(photos(&mut conn, &listed[0].id).unwrap().is_empty());

        let loose = create_batch(&mut conn, None).unwrap();
        assert_eq!(loose.parent_id, None);
    }

    #[test]
    fn open_batches_lists_unfinished_batches_of_the_parent_newest_first() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let (older, _) = ingest_batch(&mut conn, Some(&ids.garage));
        let (newer, _) = ingest_batch(&mut conn, Some(&ids.garage));
        let (finished, _) = ingest_batch(&mut conn, Some(&ids.garage));
        let (elsewhere, _) = ingest_batch(&mut conn, Some(&ids.house));
        let (loose, _) = ingest_batch(&mut conn, None);
        diesel::update(ingest_batches::table.find(&older.id))
            .set(ingest_batches::created_at.eq(at(1)))
            .execute(&mut conn)
            .unwrap();
        diesel::update(ingest_batches::table.find(&finished.id))
            .set(ingest_batches::status.eq(IngestBatchStatus::Done))
            .execute(&mut conn)
            .unwrap();

        let ids_of = |batches: Vec<IngestBatch>| -> Vec<String> {
            batches.into_iter().map(|b| b.id).collect()
        };
        assert_eq!(
            ids_of(open_batches(&mut conn, Some(&ids.garage)).unwrap()),
            [newer.id, older.id]
        );
        assert_eq!(
            ids_of(open_batches(&mut conn, Some(&ids.house)).unwrap()),
            [elsewhere.id]
        );
        assert_eq!(ids_of(open_batches(&mut conn, None).unwrap()), [loose.id]);
    }

    #[test]
    fn add_item_positions_increase_and_only_while_collecting() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (data, _) = data_with(&[]);
        let (batch, first) = ingest_batch(&mut conn, None);

        let second = add_item(&mut conn, &batch.id).unwrap();
        let third = add_item(&mut conn, &batch.id).unwrap();
        assert_eq!((second.position, third.position), (1, 2));
        assert_eq!(second.status, IngestItemStatus::Collecting);
        remove_item(&mut conn, data.path(), &second.id).unwrap();
        assert_eq!(add_item(&mut conn, &batch.id).unwrap().position, 3);
        assert_eq!(
            refusal(add_item(&mut conn, "missing")),
            IngestError::NotFound(IngestRecord::Batch)
        );

        let photo = ingest_photo(&mut conn, &first.id, &"cc".repeat(32), "image/jpeg");
        let next = ingest_photo(&mut conn, &first.id, &"dd".repeat(32), "image/png");
        assert_eq!((photo.position, next.position), (0, 1));
        assert_eq!(photo.status, IngestPhotoStatus::Pending);
        submit(&mut conn, &batch.id).unwrap();

        assert_eq!(
            refusal(add_item(&mut conn, &batch.id)),
            IngestError::NotCollecting
        );
        assert_eq!(
            refusal(insert_photo_row(
                &mut conn,
                &first.id,
                "ee",
                "image/jpeg",
                1,
                "late.jpg"
            )),
            IngestError::NotCollecting
        );
        assert_eq!(
            refusal(remove_photo(&mut conn, data.path(), &photo.id)),
            IngestError::NotCollecting
        );
        assert_eq!(
            refusal(remove_item(&mut conn, data.path(), &first.id)),
            IngestError::NotCollecting
        );
        assert_eq!(photos(&mut conn, &first.id).unwrap().len(), 2);
    }

    #[test]
    fn submit_drops_empty_items_and_refuses_an_empty_batch() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (batch, first) = ingest_batch(&mut conn, None);
        let second = add_item(&mut conn, &batch.id).unwrap();

        assert_eq!(refusal(submit(&mut conn, &batch.id)), IngestError::Empty);
        // The refusal changes nothing: both items are still there to fill.
        assert_eq!(items(&mut conn, &batch.id).unwrap().len(), 2);
        assert_eq!(
            batch_status(&mut conn, &batch.id),
            IngestBatchStatus::Collecting
        );

        ingest_photo(&mut conn, &second.id, &"cc".repeat(32), "image/jpeg");
        submit(&mut conn, &batch.id).unwrap();
        let kept: Vec<String> = items(&mut conn, &batch.id)
            .unwrap()
            .into_iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(kept, [second.id]);
        assert!(get_item(&mut conn, &first.id).unwrap().is_none());
    }

    #[test]
    fn submit_queues_items_and_starts_processing() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (batch, first) = ingest_batch(&mut conn, None);
        let second = add_item(&mut conn, &batch.id).unwrap();
        ingest_photo(&mut conn, &first.id, &"cc".repeat(32), "image/jpeg");
        ingest_photo(&mut conn, &second.id, &"dd".repeat(32), "image/jpeg");

        let submitted = submit(&mut conn, &batch.id).unwrap();

        assert_eq!(submitted.status, IngestBatchStatus::Processing);
        assert!(submitted.updated_at >= batch.updated_at);
        assert_eq!(
            statuses(&mut conn, &batch.id),
            [IngestItemStatus::Queued, IngestItemStatus::Queued]
        );
        assert_eq!(
            refusal(submit(&mut conn, &batch.id)),
            IngestError::NotCollecting
        );
    }

    #[test]
    fn accept_creates_the_entity_with_its_photos_kinds_and_primary() {
        // Review focus 4: the user's values and kinds win over the suggestion.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let (receipt_sha, photo_sha, other_sha) =
            ("cc".repeat(32), "dd".repeat(32), "ee".repeat(32));
        let (_data, files) = data_with(&[&receipt_sha, &photo_sha, &other_sha]);
        let (batch, item, photos_staged) = ready_item(
            &mut conn,
            Some(&ids.garage),
            &[&receipt_sha, &photo_sha, &other_sha],
        );
        let [receipt, photo, other] = &photos_staged[..] else {
            panic!("three photos staged");
        };
        // The model called the receipt a photo; the user corrects it.
        mark_photo_described(&mut conn, &receipt.id, &json!({}), SuggestedKind::Photo).unwrap();
        mark_photo_described(&mut conn, &other.id, &json!({}), SuggestedKind::Other).unwrap();
        set_item_suggestion(&mut conn, &item.id, &json!({ "name": "Suggested" })).unwrap();

        let kinds = [
            (receipt.id.clone(), AttachmentKind::Receipt),
            (photo.id.clone(), AttachmentKind::Photo),
        ];
        let created = accept(
            &mut conn,
            &item.id,
            entity_input("Edited name", ITEM_TYPE_ID, Some(&ids.tote_a)),
            &kinds,
        )
        .unwrap();

        assert_eq!(created.name, "Edited name");
        assert_eq!(created.parent_id.as_deref(), Some(ids.tote_a.as_str()));
        let attached: Vec<(String, AttachmentKind, bool)> =
            attachment::for_entity(&mut conn, &created.id)
                .unwrap()
                .into_iter()
                .map(|a| (a.sha256, a.kind, a.is_primary))
                .collect();
        assert_eq!(
            attached,
            [
                (photo_sha.clone(), AttachmentKind::Photo, true),
                (receipt_sha.clone(), AttachmentKind::Receipt, false),
                // No kind given: the suggested `other` is stored as an attachment.
                (other_sha.clone(), AttachmentKind::Attachment, false),
            ]
        );
        assert!(photos(&mut conn, &item.id).unwrap().is_empty());
        assert!(
            files.iter().all(|file| file.exists()),
            "accept keeps every file"
        );
        let accepted = get_item(&mut conn, &item.id).unwrap().unwrap();
        assert_eq!(accepted.status, IngestItemStatus::Accepted);
        assert_eq!(accepted.entity_id.as_deref(), Some(created.id.as_str()));
        assert_eq!(batch_status(&mut conn, &batch.id), IngestBatchStatus::Done);
    }

    #[test]
    fn accept_of_the_last_open_item_closes_the_batch() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (batch, first) = ingest_batch(&mut conn, None);
        let second = add_item(&mut conn, &batch.id).unwrap();
        ingest_photo(&mut conn, &first.id, &"cc".repeat(32), "image/jpeg");
        ingest_photo(&mut conn, &second.id, &"dd".repeat(32), "image/jpeg");
        submit(&mut conn, &batch.id).unwrap();
        let count = |conn: &mut SqliteConnection| -> i64 {
            entities::table.count().get_result(conn).unwrap()
        };
        let before = count(&mut conn);

        let queued = accept(
            &mut conn,
            &first.id,
            entity_input("A", ITEM_TYPE_ID, None),
            &[],
        );
        assert_eq!(refusal(queued), IngestError::NotReviewable);
        assert_eq!(count(&mut conn), before, "a refused accept creates nothing");

        set_item_status(&mut conn, &first.id, IngestItemStatus::Ready, None).unwrap();
        set_item_status(&mut conn, &second.id, IngestItemStatus::Failed, Some("no")).unwrap();
        settle_batch(&mut conn, &batch.id).unwrap();
        assert_eq!(
            batch_status(&mut conn, &batch.id),
            IngestBatchStatus::Reviewing
        );

        accept(
            &mut conn,
            &first.id,
            entity_input("A", ITEM_TYPE_ID, None),
            &[],
        )
        .unwrap();
        assert_eq!(
            batch_status(&mut conn, &batch.id),
            IngestBatchStatus::Reviewing
        );
        // A failed item can still be accepted, with the user's own values.
        accept(
            &mut conn,
            &second.id,
            entity_input("B", ITEM_TYPE_ID, None),
            &[],
        )
        .unwrap();
        assert_eq!(batch_status(&mut conn, &batch.id), IngestBatchStatus::Done);
        assert_eq!(count(&mut conn), before + 2);
        let again = accept(
            &mut conn,
            &second.id,
            entity_input("B", ITEM_TYPE_ID, None),
            &[],
        );
        assert_eq!(refusal(again), IngestError::NotReviewable);
    }

    #[test]
    fn accept_refuses_a_kind_for_a_photo_of_another_item() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (_, item, _) = ready_item(&mut conn, None, &[&"cc".repeat(32)]);
        let (_, _, foreign) = ready_item(&mut conn, None, &[&"dd".repeat(32)]);
        let kinds = [(foreign[0].id.clone(), AttachmentKind::Photo)];

        let result = accept(
            &mut conn,
            &item.id,
            entity_input("A", ITEM_TYPE_ID, None),
            &kinds,
        );

        assert_eq!(refusal(result), IngestError::ForeignPhoto);
        assert_eq!(photos(&mut conn, &item.id).unwrap().len(), 1);
    }

    /// Accepts ready item `item_id` as a new item with no kind overrides and
    /// returns its attachments as `(mime_type, kind, is_primary)`, primary first.
    fn accept_plainly(
        conn: &mut SqliteConnection,
        item_id: &str,
    ) -> Vec<(String, AttachmentKind, bool)> {
        let created = accept(conn, item_id, entity_input("A", ITEM_TYPE_ID, None), &[]).unwrap();
        attachment::for_entity(conn, &created.id)
            .unwrap()
            .into_iter()
            .map(|a| (a.mime_type, a.kind, a.is_primary))
            .collect()
    }

    #[test]
    fn the_primary_photo_is_the_first_that_can_be_thumbnailed() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let photo = Some(SuggestedKind::Photo);
        let item = staged_item(
            &mut conn,
            &[
                (&"cc".repeat(32), "image/heic", photo),
                (&"dd".repeat(32), "image/jpeg", photo),
            ],
        );

        assert_eq!(
            accept_plainly(&mut conn, &item),
            [
                ("image/jpeg".to_owned(), AttachmentKind::Photo, true),
                ("image/heic".to_owned(), AttachmentKind::Photo, false),
            ]
        );

        let only_heic = staged_item(&mut conn, &[(&"ee".repeat(32), "image/heic", photo)]);
        assert_eq!(
            accept_plainly(&mut conn, &only_heic),
            [("image/heic".to_owned(), AttachmentKind::Photo, false)]
        );
    }

    #[test]
    fn no_photo_kind_means_no_primary_and_no_kind_at_all_means_photo() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let receipt = Some(SuggestedKind::Receipt);
        let documents = staged_item(
            &mut conn,
            &[
                (&"cc".repeat(32), "image/jpeg", receipt),
                (&"dd".repeat(32), "image/jpeg", receipt),
            ],
        );
        assert_eq!(
            accept_plainly(&mut conn, &documents),
            [
                ("image/jpeg".to_owned(), AttachmentKind::Receipt, false),
                ("image/jpeg".to_owned(), AttachmentKind::Receipt, false),
            ]
        );

        // A photo the vision step never described has no suggested kind.
        let undescribed = staged_item(&mut conn, &[(&"ee".repeat(32), "image/jpeg", None)]);
        assert_eq!(
            accept_plainly(&mut conn, &undescribed),
            [("image/jpeg".to_owned(), AttachmentKind::Photo, true)]
        );
    }

    #[test]
    fn skip_removes_the_photos_files_unless_shared() {
        // Review focus 1: bytes an attachment also holds must stay.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let own_sha = "cc".repeat(32);
        let (data, files) = data_with(&[&ids.photo_sha256, &own_sha]);
        let (batch, item, _) = ready_item(&mut conn, None, &[&ids.photo_sha256, &own_sha]);
        attachment::insert_thumbnail(&mut conn, &thumb(&own_sha)).unwrap();

        let skipped = skip(&mut conn, data.path(), &item.id).unwrap();

        assert_eq!(skipped.status, IngestItemStatus::Skipped);
        assert!(photos(&mut conn, &item.id).unwrap().is_empty());
        assert!(files[0].exists(), "the attachment's original must stay");
        assert!(!files[1].exists(), "an unshared original must go");
        assert!(
            attachment::thumbnail(&mut conn, &own_sha, 300)
                .unwrap()
                .is_none()
        );
        assert_eq!(batch_status(&mut conn, &batch.id), IngestBatchStatus::Done);
        assert_eq!(
            refusal(skip(&mut conn, data.path(), &item.id)),
            IngestError::NotReviewable
        );
    }

    #[test]
    fn remove_and_delete_batch_remove_unshared_files() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let shas = ["cc", "dd", "ee"].map(|pair| pair.repeat(32));
        let (data, files) = data_with(&[&shas[0], &shas[1], &shas[2]]);
        let (batch, first) = ingest_batch(&mut conn, None);
        let second = add_item(&mut conn, &batch.id).unwrap();
        let photo = ingest_photo(&mut conn, &first.id, &shas[0], "image/jpeg");
        // The same bytes staged twice share one file.
        ingest_photo(&mut conn, &first.id, &shas[1], "image/jpeg");
        ingest_photo(&mut conn, &second.id, &shas[1], "image/jpeg");
        ingest_photo(&mut conn, &second.id, &shas[2], "image/jpeg");

        remove_photo(&mut conn, data.path(), &photo.id).unwrap();
        assert!(!files[0].exists());
        assert!(get_photo(&mut conn, &photo.id).unwrap().is_none());
        remove_item(&mut conn, data.path(), &first.id).unwrap();
        assert!(
            files[1].exists(),
            "the second item still stages these bytes"
        );
        assert_eq!(
            refusal(remove_photo(&mut conn, data.path(), &photo.id)),
            IngestError::NotFound(IngestRecord::Photo)
        );

        delete_batch(&mut conn, data.path(), &batch.id).unwrap();
        assert!(files.iter().all(|file| !file.exists()));
        assert!(get_batch(&mut conn, &batch.id).unwrap().is_none());
        assert!(get_item(&mut conn, &second.id).unwrap().is_none());
        let staged: i64 = ingest_photos::table.count().get_result(&mut conn).unwrap();
        assert_eq!(staged, 0);
    }

    #[test]
    fn cleanup_removes_only_stale_batches_and_their_files() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (stale_sha, fresh_sha) = ("cc".repeat(32), "dd".repeat(32));
        let (data, files) = data_with(&[&stale_sha, &fresh_sha]);
        let (stale, stale_item) = ingest_batch(&mut conn, None);
        ingest_photo(&mut conn, &stale_item.id, &stale_sha, "image/jpeg");
        let (fresh, fresh_item) = ingest_batch(&mut conn, None);
        ingest_photo(&mut conn, &fresh_item.id, &fresh_sha, "image/jpeg");
        let now = Utc::now().naive_utc();
        diesel::update(ingest_batches::table.find(&stale.id))
            .set(ingest_batches::updated_at.eq(now - TimeDelta::days(8)))
            .execute(&mut conn)
            .unwrap();

        let removed = cleanup_stale(&mut conn, data.path(), now - TimeDelta::days(7)).unwrap();

        assert_eq!(removed, 1);
        assert!(get_batch(&mut conn, &stale.id).unwrap().is_none());
        assert!(get_item(&mut conn, &stale_item.id).unwrap().is_none());
        assert!(!files[0].exists());
        assert!(get_batch(&mut conn, &fresh.id).unwrap().is_some());
        assert_eq!(photos(&mut conn, &fresh_item.id).unwrap().len(), 1);
        assert!(files[1].exists());
    }

    #[test]
    fn activity_bumps_the_batch_so_cleanup_spares_it() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (data, _) = data_with(&[]);
        let (batch, item) = ingest_batch(&mut conn, None);
        let long_ago = Utc::now().naive_utc() - TimeDelta::days(30);
        let age = |conn: &mut SqliteConnection| {
            diesel::update(ingest_batches::table.find(&batch.id))
                .set(ingest_batches::updated_at.eq(long_ago))
                .execute(conn)
                .unwrap();
        };
        let cutoff = Utc::now().naive_utc() - TimeDelta::days(7);

        age(&mut conn);
        touch(&mut conn, &batch.id).unwrap();
        assert_eq!(cleanup_stale(&mut conn, data.path(), cutoff).unwrap(), 0);
        age(&mut conn);
        ingest_photo(&mut conn, &item.id, &"cc".repeat(32), "image/jpeg");
        assert_eq!(cleanup_stale(&mut conn, data.path(), cutoff).unwrap(), 0);
        submit(&mut conn, &batch.id).unwrap();
        age(&mut conn);
        set_item_status(&mut conn, &item.id, IngestItemStatus::Analysing, None).unwrap();
        assert_eq!(cleanup_stale(&mut conn, data.path(), cutoff).unwrap(), 0);

        // Reviewing an item a day keeps a batch alive even when an accept or
        // skip leaves its status unchanged.
        let later = ready_item_in(&mut conn, &batch.id);
        set_item_status(&mut conn, &item.id, IngestItemStatus::Ready, None).unwrap();
        settle_batch(&mut conn, &batch.id).unwrap();
        age(&mut conn);
        accept(
            &mut conn,
            &item.id,
            entity_input("A", ITEM_TYPE_ID, None),
            &[],
        )
        .unwrap();
        assert_eq!(
            batch_status(&mut conn, &batch.id),
            IngestBatchStatus::Reviewing
        );
        assert_eq!(cleanup_stale(&mut conn, data.path(), cutoff).unwrap(), 0);
        let third = ready_item_in(&mut conn, &batch.id);
        age(&mut conn);
        skip(&mut conn, data.path(), &later).unwrap();
        assert_eq!(
            batch_status(&mut conn, &batch.id),
            IngestBatchStatus::Reviewing
        );
        assert_eq!(cleanup_stale(&mut conn, data.path(), cutoff).unwrap(), 0);
        assert!(get_item(&mut conn, &third).unwrap().is_some());
        assert_eq!(
            refusal(touch(&mut conn, "missing")),
            IngestError::NotFound(IngestRecord::Batch)
        );
    }

    #[test]
    fn requeue_interrupted_resets_analysing_items() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (running, first) = ingest_batch(&mut conn, None);
        let second = add_item(&mut conn, &running.id).unwrap();
        ingest_photo(&mut conn, &first.id, &"cc".repeat(32), "image/jpeg");
        ingest_photo(&mut conn, &second.id, &"dd".repeat(32), "image/jpeg");
        submit(&mut conn, &running.id).unwrap();
        set_item_status(&mut conn, &first.id, IngestItemStatus::Analysing, None).unwrap();
        set_item_status(&mut conn, &second.id, IngestItemStatus::Ready, None).unwrap();
        let (collecting, _) = ingest_batch(&mut conn, None);
        let (_, reviewed, _) = ready_item(&mut conn, None, &[&"ee".repeat(32)]);
        let reviewed_batch = get_item(&mut conn, &reviewed.id).unwrap().unwrap().batch_id;
        settle_batch(&mut conn, &reviewed_batch).unwrap();

        let restart = requeue_interrupted(&mut conn).unwrap();
        let again = requeue_interrupted(&mut conn).unwrap();

        assert_eq!(restart, [running.id.as_str()]);
        // Nothing is left analysing, so a second call only lists the batch.
        assert_eq!(again, restart);
        assert_eq!(
            statuses(&mut conn, &running.id),
            [IngestItemStatus::Queued, IngestItemStatus::Ready]
        );
        assert_eq!(
            statuses(&mut conn, &collecting.id),
            [IngestItemStatus::Collecting]
        );
        assert_eq!(
            statuses(&mut conn, &reviewed_batch),
            [IngestItemStatus::Ready]
        );
    }

    #[test]
    fn retry_requeues_a_reviewable_item_and_reopens_its_batch() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (batch, item, _) = ready_item(&mut conn, None, &[&"cc".repeat(32)]);
        set_item_status(
            &mut conn,
            &item.id,
            IngestItemStatus::Failed,
            Some("no answer"),
        )
        .unwrap();
        settle_batch(&mut conn, &batch.id).unwrap();
        assert_eq!(
            batch_status(&mut conn, &batch.id),
            IngestBatchStatus::Reviewing
        );

        let retried = retry(&mut conn, &item.id).unwrap();

        assert_eq!(retried.status, IngestItemStatus::Queued);
        assert_eq!(retried.error, None);
        assert_eq!(
            batch_status(&mut conn, &batch.id),
            IngestBatchStatus::Processing
        );
        assert_eq!(
            refusal(retry(&mut conn, &item.id)),
            IngestError::NotRetryable
        );
        let (collecting, open) = ingest_batch(&mut conn, None);
        assert_eq!(
            refusal(retry(&mut conn, &open.id)),
            IngestError::NotRetryable
        );
        assert_eq!(
            batch_status(&mut conn, &collecting.id),
            IngestBatchStatus::Collecting
        );
        assert_eq!(
            refusal(retry(&mut conn, "missing")),
            IngestError::NotFound(IngestRecord::Item)
        );
    }

    #[test]
    fn settle_moves_processing_to_reviewing_and_to_done() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (batch, first) = ingest_batch(&mut conn, None);
        assert_eq!(
            settle_batch(&mut conn, &batch.id).unwrap().status,
            IngestBatchStatus::Collecting,
            "a collecting batch is not settled"
        );
        let second = add_item(&mut conn, &batch.id).unwrap();
        ingest_photo(&mut conn, &first.id, &"cc".repeat(32), "image/jpeg");
        ingest_photo(&mut conn, &second.id, &"dd".repeat(32), "image/jpeg");
        submit(&mut conn, &batch.id).unwrap();
        let settle = |conn: &mut SqliteConnection| settle_batch(conn, &batch.id).unwrap().status;

        assert_eq!(settle(&mut conn), IngestBatchStatus::Processing);
        set_item_status(&mut conn, &first.id, IngestItemStatus::Ready, None).unwrap();
        set_item_status(&mut conn, &second.id, IngestItemStatus::Analysing, None).unwrap();
        assert_eq!(settle(&mut conn), IngestBatchStatus::Processing);
        set_item_status(
            &mut conn,
            &second.id,
            IngestItemStatus::Failed,
            Some("no answer"),
        )
        .unwrap();
        assert_eq!(settle(&mut conn), IngestBatchStatus::Reviewing);
        let failed = get_item(&mut conn, &second.id).unwrap().unwrap();
        assert_eq!(failed.error.as_deref(), Some("no answer"));
        set_item_status(&mut conn, &first.id, IngestItemStatus::Accepted, None).unwrap();
        assert_eq!(settle(&mut conn), IngestBatchStatus::Reviewing);
        set_item_status(&mut conn, &second.id, IngestItemStatus::Skipped, None).unwrap();
        assert_eq!(settle(&mut conn), IngestBatchStatus::Done);
        assert_eq!(
            get_item(&mut conn, &second.id).unwrap().unwrap().error,
            None
        );
    }

    #[test]
    fn marks_record_descriptions_failures_and_suggestions() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let (_, item, staged) = ready_item(&mut conn, None, &[&"cc".repeat(32), &"dd".repeat(32)]);
        let description = json!({ "kind": "receipt", "summary": "A receipt" });

        mark_photo_described(
            &mut conn,
            &staged[0].id,
            &description,
            SuggestedKind::Receipt,
        )
        .unwrap();
        mark_photo_failed(&mut conn, &staged[1].id, "unreadable answer").unwrap();
        set_item_suggestion(&mut conn, &item.id, &json!({ "name": "Mouse" })).unwrap();

        let described = get_photo(&mut conn, &staged[0].id).unwrap().unwrap();
        assert_eq!(described.status, IngestPhotoStatus::Described);
        assert_eq!(described.suggested_kind, Some(SuggestedKind::Receipt));
        let stored: Value =
            serde_json::from_str(described.description.as_deref().unwrap()).unwrap();
        assert_eq!(stored, description);
        let failed = get_photo(&mut conn, &staged[1].id).unwrap().unwrap();
        assert_eq!(failed.status, IngestPhotoStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("unreadable answer"));
        let suggested = get_item(&mut conn, &item.id).unwrap().unwrap();
        assert_eq!(suggested.suggestion.as_deref(), Some(r#"{"name":"Mouse"}"#));
        assert_eq!(
            refusal(mark_photo_failed(&mut conn, "missing", "x")),
            IngestError::NotFound(IngestRecord::Photo)
        );
        assert_eq!(
            refusal(set_item_status(
                &mut conn,
                "missing",
                IngestItemStatus::Ready,
                None
            )),
            IngestError::NotFound(IngestRecord::Item)
        );
    }

    /// A 300px thumbnail row for `sha256`.
    fn thumb(sha256: &str) -> Thumbnail {
        Thumbnail {
            sha256: sha256.to_owned(),
            size: 300,
            mime_type: "image/webp".to_owned(),
            width: 4,
            height: 3,
            data: vec![0; 8],
            created_at: at(0),
        }
    }
}
