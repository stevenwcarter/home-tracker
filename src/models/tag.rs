use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::schema::{tag_entities, tags};

/// A label that can be attached to any number of entities; tags nest via `parent_id`.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = tags, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// One tag assignment. The row is its own key, so there is nothing to update.
#[derive(Debug, Clone, PartialEq, Queryable, Insertable)]
#[diesel(table_name = tag_entities, check_for_backend(diesel::sqlite::Sqlite))]
pub struct TagEntity {
    pub tag_id: String,
    pub entity_id: String,
}
