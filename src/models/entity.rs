use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;

use crate::asset_id::AssetId;
use crate::money::Cents;
use crate::schema::entities;

/// A location or an item; which one is decided by its type's `is_location`.
#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = entities, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_null = true)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub entity_type_id: String,
    pub parent_id: Option<String>,
    pub archived: bool,
    pub asset_id: AssetId,
    pub import_ref: Option<String>,
    pub notes: Option<String>,
    pub quantity: f64,
    pub insured: bool,
    pub serial_number: Option<String>,
    pub model_number: Option<String>,
    pub manufacturer: Option<String>,
    pub lifetime_warranty: bool,
    pub warranty_expires: Option<NaiveDate>,
    pub warranty_details: Option<String>,
    pub purchase_date: Option<NaiveDate>,
    pub purchase_from: Option<String>,
    pub purchase_price_cents: Cents,
    pub sold_date: Option<NaiveDate>,
    pub sold_to: Option<String>,
    pub sold_price_cents: Cents,
    pub sold_notes: Option<String>,
    pub sync_child_entity_locations: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
