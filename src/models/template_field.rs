use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::kinds::FieldKind;
use crate::schema::template_fields;

/// A custom field a template pre-fills on entities created from it.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = template_fields, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct TemplateField {
    pub id: String,
    pub template_id: String,
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
