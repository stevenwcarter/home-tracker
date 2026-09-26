use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::kinds::{IngestBatchStatus, IngestItemStatus, IngestPhotoStatus, SuggestedKind};
use crate::schema::{ingest_batches, ingest_items, ingest_photos};

/// A group of items being staged for AI ingest, started from `parent_id`
/// (the entity the new items go under by default).
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable)]
#[diesel(table_name = ingest_batches, check_for_backend(diesel::sqlite::Sqlite))]
pub struct IngestBatch {
    pub id: String,
    pub parent_id: Option<String>,
    pub status: IngestBatchStatus,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// One future entity of a batch: its photos, the model's suggestion (JSON)
/// once synthesised, and the entity it became once accepted.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable)]
#[diesel(table_name = ingest_items, check_for_backend(diesel::sqlite::Sqlite))]
pub struct IngestItem {
    pub id: String,
    pub batch_id: String,
    pub position: i32,
    pub status: IngestItemStatus,
    pub error: Option<String>,
    pub suggestion: Option<String>,
    pub entity_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// A staged photo of an item. Its bytes live in the originals store under
/// `sha256`, shared with any attachment of the same bytes.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable)]
#[diesel(table_name = ingest_photos, check_for_backend(diesel::sqlite::Sqlite))]
pub struct IngestPhoto {
    pub id: String,
    pub item_id: String,
    pub position: i32,
    pub sha256: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub title: String,
    pub status: IngestPhotoStatus,
    pub error: Option<String>,
    pub description: Option<String>,
    pub suggested_kind: Option<SuggestedKind>,
    pub created_at: NaiveDateTime,
}
