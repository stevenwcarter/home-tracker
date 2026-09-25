use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::kinds::AttachmentKind;
use crate::schema::attachments;

/// Metadata for a stored file; the bytes live under `$DATA_DIR/originals/<sha256>`.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = attachments, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct Attachment {
    pub id: String,
    pub entity_id: String,
    pub kind: AttachmentKind,
    pub is_primary: bool,
    pub title: String,
    pub mime_type: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
