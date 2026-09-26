use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::schema::thumbnails;

/// A resized rendition of an original, keyed by `(sha256, size)`: every
/// attachment whose bytes hash to `sha256` shares it.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = thumbnails, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(primary_key(sha256, size), treat_none_as_null = true)]
pub struct Thumbnail {
    pub sha256: String,
    pub size: i32,
    pub mime_type: String,
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
    pub created_at: NaiveDateTime,
}
