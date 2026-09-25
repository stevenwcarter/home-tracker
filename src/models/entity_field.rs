use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::kinds::FieldKind;
use crate::schema::entity_fields;

/// A user-defined custom field on one entity; `kind` says which value column is meaningful.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = entity_fields, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct EntityField {
    pub id: String,
    pub entity_id: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: FieldKind,
    pub text_value: Option<String>,
    pub number_value: Option<i64>,
    pub boolean_value: bool,
    pub time_value: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
