use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::schema::thumbnails;

/// A resized rendition of a photo attachment, keyed by `(attachment_id, size)`.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = thumbnails, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(primary_key(attachment_id, size), treat_none_as_null = true)]
pub struct Thumbnail {
    pub attachment_id: String,
    pub size: i32,
    pub mime_type: String,
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
    pub created_at: NaiveDateTime,
}
