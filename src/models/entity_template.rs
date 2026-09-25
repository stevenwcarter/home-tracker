use chrono::NaiveDateTime;
use diesel::prelude::*;

use crate::schema::entity_templates;

/// Defaults applied when creating an entity from a template.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = entity_templates, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct EntityTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub default_quantity: f64,
    pub default_insured: bool,
    pub default_name: Option<String>,
    pub default_description: Option<String>,
    pub default_manufacturer: Option<String>,
    pub default_model_number: Option<String>,
    pub default_lifetime_warranty: bool,
    pub default_warranty_details: Option<String>,
    pub include_warranty_fields: bool,
    pub include_purchase_fields: bool,
    pub include_sold_fields: bool,
    /// JSON array of tag ids, stored verbatim from Homebox.
    pub default_tag_ids: Option<String>,
    pub location_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
