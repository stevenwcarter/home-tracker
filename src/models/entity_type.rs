use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::schema::entity_types;

/// A kind of entity (`Location`, `Item`, or a user-defined type such as `Tote`).
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = entity_types, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct EntityType {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub is_location: bool,
    pub default_template_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
