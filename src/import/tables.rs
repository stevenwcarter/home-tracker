//! The JSON shapes inside a Homebox backup (schemaVersion 1). Column names are
//! Homebox's, including the misleading `entity_children` (it holds the PARENT id).
//! Unknown extra keys (`group_entities`, `group_tags`, …) are ignored.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Deserializer};

/// `manifest.json`: identifies the export format and summarises its row counts.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: i64,
    #[serde(default)]
    pub exported_at: Option<String>,
    #[serde(default)]
    pub homebox_version: Option<String>,
    #[serde(default)]
    pub counts: HashMap<String, i64>,
}

/// Homebox writes SQLite booleans as 0/1 and Postgres booleans as true/false.
pub fn bool_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        B(bool),
        I(i64),
        S(String),
    }
    Ok(match Raw::deserialize(d)? {
        Raw::B(b) => b,
        Raw::I(i) => i != 0,
        Raw::S(s) => matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "t"),
    })
}

/// `"2026-08-27T16:00:08Z"` (any RFC 3339 offset, any precision) → naive UTC.
pub fn parse_timestamp(s: &str) -> anyhow::Result<NaiveDateTime> {
    Ok(DateTime::parse_from_rfc3339(s)?.naive_utc())
}

/// The date part of an RFC 3339 timestamp, or a bare `YYYY-MM-DD`.
pub fn parse_date(s: &str) -> anyhow::Result<NaiveDate> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.date_naive());
    }
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
}

/// Homebox stores "no value" as either `null` or `""`; both become `None`.
fn empty_as_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|s| !s.is_empty()))
}

fn one() -> f64 {
    1.0
}

fn octet_stream() -> String {
    "application/octet-stream".to_owned()
}

/// One row of `entity_types.json`.
#[derive(Debug, Deserialize)]
pub struct EntityTypeRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub icon: Option<String>,
    #[serde(deserialize_with = "bool_lenient")]
    pub is_location: bool,
    #[serde(default)]
    pub entity_type_default_template: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `tags.json`; `tag_children` holds the PARENT tag id.
#[derive(Debug, Deserialize)]
pub struct TagRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub tag_children: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `entities.json`; `entity_children` holds the PARENT entity id.
#[derive(Debug, Deserialize)]
pub struct EntityRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    pub entity_type_entities: String,
    #[serde(default)]
    pub entity_children: Option<String>,
    #[serde(deserialize_with = "bool_lenient")]
    pub archived: bool,
    #[serde(default)]
    pub asset_id: i64,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub import_ref: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub notes: Option<String>,
    #[serde(default = "one")]
    pub quantity: f64,
    #[serde(deserialize_with = "bool_lenient")]
    pub insured: bool,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub serial_number: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub model_number: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub manufacturer: Option<String>,
    #[serde(deserialize_with = "bool_lenient")]
    pub lifetime_warranty: bool,
    #[serde(default)]
    pub warranty_expires: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub warranty_details: Option<String>,
    #[serde(default)]
    pub purchase_date: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub purchase_from: Option<String>,
    #[serde(default)]
    pub purchase_price: f64,
    #[serde(default)]
    pub sold_date: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub sold_to: Option<String>,
    #[serde(default)]
    pub sold_price: f64,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub sold_notes: Option<String>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub sync_child_entity_locations: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `entity_fields.json`; `entity_fields` is the owning entity id.
#[derive(Debug, Deserialize)]
pub struct EntityFieldRow {
    pub id: String,
    pub entity_fields: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text_value: Option<String>,
    #[serde(default)]
    pub number_value: Option<i64>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub boolean_value: bool,
    #[serde(default)]
    pub time_value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `entity_templates.json`.
#[derive(Debug, Deserialize)]
pub struct EntityTemplateRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub notes: Option<String>,
    #[serde(default = "one")]
    pub default_quantity: f64,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub default_insured: bool,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_name: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_manufacturer: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_model_number: Option<String>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub default_lifetime_warranty: bool,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_warranty_details: Option<String>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub include_warranty_fields: bool,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub include_purchase_fields: bool,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub include_sold_fields: bool,
    #[serde(default)]
    pub default_tag_ids: Option<serde_json::Value>,
    #[serde(default)]
    pub entity_template_location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `template_fields.json`; `entity_template_fields` is the owning template id.
#[derive(Debug, Deserialize)]
pub struct TemplateFieldRow {
    pub id: String,
    pub entity_template_fields: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text_value: Option<String>,
    #[serde(default)]
    pub number_value: Option<i64>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub boolean_value: bool,
    #[serde(default)]
    pub time_value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `attachments.json`. Thumbnail rows (`type: "thumbnail"`) have no
/// owning entity and are referenced from their photo's `attachment_thumbnail`.
#[derive(Debug, Deserialize)]
pub struct AttachmentRow {
    pub id: String,
    #[serde(default)]
    pub entity_attachments: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub primary: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub path: String,
    #[serde(default = "octet_stream")]
    pub mime_type: String,
    #[serde(default)]
    pub attachment_thumbnail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `tag_entities.json`.
#[derive(Debug, Deserialize)]
pub struct TagEntityRow {
    pub tag_id: String,
    pub entity_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn booleans_accept_ints_bools_and_strings() {
        #[derive(Deserialize)]
        struct T {
            #[serde(deserialize_with = "bool_lenient")]
            v: bool,
        }
        for (raw, want) in [
            ("1", true),
            ("0", false),
            ("true", true),
            ("false", false),
            ("\"t\"", true),
            ("\"0\"", false),
        ] {
            let t: T = serde_json::from_str(&format!("{{\"v\":{raw}}}")).unwrap();
            assert_eq!(t.v, want, "{raw}");
        }
    }

    #[test]
    fn timestamps_and_dates_parse_homebox_shapes() {
        assert_eq!(
            parse_timestamp("2026-08-27T16:00:08Z").unwrap().to_string(),
            "2026-08-27 16:00:08"
        );
        assert_eq!(
            parse_timestamp("2026-09-15T02:47:58.179502537Z")
                .unwrap()
                .to_string(),
            "2026-09-15 02:47:58.179502537"
        );
        assert_eq!(
            parse_timestamp("2026-09-15T02:47:58+02:00")
                .unwrap()
                .to_string(),
            "2026-09-15 00:47:58"
        );
        assert_eq!(
            parse_date("2021-10-23T00:00:00Z").unwrap().to_string(),
            "2021-10-23"
        );
        assert_eq!(parse_date("2021-10-23").unwrap().to_string(), "2021-10-23");
        assert!(parse_date("nope").is_err());
    }

    #[test]
    fn an_entity_row_from_the_real_export_shape_deserializes() {
        let raw = r#"{"archived":0,"asset_id":7,"created_at":"2024-09-13T12:23:29.134012559Z","description":"","entity_children":null,"entity_type_entities":"t1","group_entities":"g","id":"e1","import_ref":null,"insured":0,"lifetime_warranty":0,"manufacturer":null,"model_number":null,"name":"Attic","notes":null,"purchase_date":null,"purchase_from":null,"purchase_price":0,"quantity":1,"serial_number":null,"sold_date":null,"sold_notes":null,"sold_price":0,"sold_to":null,"sync_child_entity_locations":0,"updated_at":"2026-08-27T16:00:11.501644955Z","warranty_details":null,"warranty_expires":null}"#;
        let row: EntityRow = serde_json::from_str(raw).unwrap();
        assert_eq!(row.asset_id, 7);
        assert_eq!(row.description, None);
        assert_eq!(row.entity_children, None);
        assert!(!row.archived);
    }
}
