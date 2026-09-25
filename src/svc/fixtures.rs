//! The shared sample inventory used by service, stats and GraphQL tests.
//!
//! Expected statistics: Total Value 67297 cents (15299×1 + 999×2 + 50000×1;
//! Broken lamp is archived, Loose item is free, locations are excluded),
//! 4 items (Drill, Screws, Old TV, Loose item), 4 locations (House, Garage,
//! Tote A, Attic) and 2 tags. Test-only code: failures panic.

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use diesel::prelude::*;

use crate::asset_id::AssetId;
use crate::db::{ITEM_TYPE_ID, LOCATION_TYPE_ID};
use crate::kinds::{AttachmentKind, FieldKind};
use crate::models::{Attachment, Entity, EntityField, EntityType, Tag, TagEntity, Thumbnail};
use crate::money::Cents;
use crate::schema::{
    attachments, entities, entity_fields, entity_types, tag_entities, tags, thumbnails,
};

/// Ids of every row [`seed_sample`] inserts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleIds {
    pub house: String,
    pub garage: String,
    pub tote_a: String,
    pub attic: String,
    pub drill: String,
    pub screws: String,
    pub old_tv: String,
    pub broken_lamp: String,
    pub loose: String,
    pub tools: String,
    pub electronics: String,
    pub photo: String,
    pub manual: String,
    pub tote_type: String,
}

/// 2025-01-01T00:00:00 plus `seconds`; rows are spaced one second apart so
/// ordering by `created_at` is deterministic.
pub fn at(seconds: i64) -> NaiveDateTime {
    DateTime::from_timestamp(1_735_689_600 + seconds, 0)
        .expect("fixture timestamp in range")
        .naive_utc()
}

/// An entity with every optional column empty and quantity 1, created at `at(seq)`.
pub fn entity(id: &str, name: &str, type_id: &str, parent: Option<&str>, seq: i64) -> Entity {
    Entity {
        id: id.to_owned(),
        name: name.to_owned(),
        description: None,
        entity_type_id: type_id.to_owned(),
        parent_id: parent.map(str::to_owned),
        archived: false,
        asset_id: AssetId::NONE,
        import_ref: None,
        notes: None,
        quantity: 1.0,
        insured: false,
        serial_number: None,
        model_number: None,
        manufacturer: None,
        lifetime_warranty: false,
        warranty_expires: None,
        warranty_details: None,
        purchase_date: None,
        purchase_from: None,
        purchase_price_cents: Cents(0),
        sold_date: None,
        sold_to: None,
        sold_price_cents: Cents(0),
        sold_notes: None,
        sync_child_entity_locations: false,
        created_at: at(seq),
        updated_at: at(seq),
    }
}

/// A photo or document attachment on `entity_id`, created at `at(seq)`.
pub fn attachment(
    id: &str,
    entity_id: &str,
    kind: AttachmentKind,
    is_primary: bool,
    sha256: &str,
    size_bytes: i64,
    seq: i64,
) -> Attachment {
    let mime_type = match kind {
        AttachmentKind::Photo => "image/jpeg",
        _ => "application/pdf",
    };
    Attachment {
        id: id.to_owned(),
        entity_id: entity_id.to_owned(),
        kind,
        is_primary,
        title: String::new(),
        mime_type: mime_type.to_owned(),
        sha256: sha256.to_owned(),
        size_bytes,
        created_at: at(seq),
        updated_at: at(seq),
    }
}

fn tag(id: &str, name: &str, parent: Option<&str>, seq: i64) -> Tag {
    Tag {
        id: id.to_owned(),
        name: name.to_owned(),
        description: None,
        color: None,
        icon: None,
        parent_id: parent.map(str::to_owned),
        created_at: at(seq),
        updated_at: at(seq),
    }
}

/// Inserts the sample inventory into a freshly migrated database.
pub fn seed_sample(conn: &mut SqliteConnection) -> SampleIds {
    let ids = SampleIds {
        house: "e-house".to_owned(),
        garage: "e-garage".to_owned(),
        tote_a: "e-tote-a".to_owned(),
        attic: "e-attic".to_owned(),
        drill: "e-drill".to_owned(),
        screws: "e-screws".to_owned(),
        old_tv: "e-old-tv".to_owned(),
        broken_lamp: "e-broken-lamp".to_owned(),
        loose: "e-loose".to_owned(),
        tools: "t-tools".to_owned(),
        electronics: "t-electronics".to_owned(),
        photo: "a-drill-photo".to_owned(),
        manual: "a-drill-manual".to_owned(),
        tote_type: "t-tote".to_owned(),
    };

    diesel::insert_into(entity_types::table)
        .values(EntityType {
            id: ids.tote_type.clone(),
            name: "Tote".to_owned(),
            description: None,
            icon: None,
            is_location: true,
            default_template_id: None,
            created_at: at(0),
            updated_at: at(0),
        })
        .execute(conn)
        .expect("insert Tote type");

    let house = Some(ids.house.as_str());
    let garage = Some(ids.garage.as_str());
    let rows = vec![
        entity(&ids.house, "House", LOCATION_TYPE_ID, None, 1),
        entity(&ids.garage, "Garage", LOCATION_TYPE_ID, house, 2),
        entity(&ids.tote_a, "Tote A", &ids.tote_type, garage, 3),
        Entity {
            archived: true,
            ..entity(&ids.attic, "Attic", LOCATION_TYPE_ID, None, 4)
        },
        Entity {
            purchase_price_cents: Cents(15_299),
            asset_id: AssetId(3),
            ..entity(&ids.drill, "Drill", ITEM_TYPE_ID, garage, 5)
        },
        Entity {
            purchase_price_cents: Cents(999),
            quantity: 2.0,
            asset_id: AssetId(4),
            ..entity(&ids.screws, "Screws", ITEM_TYPE_ID, Some(&ids.tote_a), 6)
        },
        Entity {
            purchase_price_cents: Cents(50_000),
            sold_price_cents: Cents(20_000),
            sold_date: NaiveDate::from_ymd_opt(2025, 1, 15),
            asset_id: AssetId(5),
            ..entity(&ids.old_tv, "Old TV", ITEM_TYPE_ID, house, 7)
        },
        Entity {
            archived: true,
            purchase_price_cents: Cents(1_000),
            ..entity(&ids.broken_lamp, "Broken lamp", ITEM_TYPE_ID, house, 8)
        },
        entity(&ids.loose, "Loose item", ITEM_TYPE_ID, None, 9),
    ];
    diesel::insert_into(entities::table)
        .values(&rows)
        .execute(conn)
        .expect("insert sample entities");

    diesel::insert_into(tags::table)
        .values(&vec![
            tag(&ids.tools, "Tools", None, 10),
            tag(&ids.electronics, "Electronics", Some(&ids.tools), 11),
        ])
        .execute(conn)
        .expect("insert sample tags");
    diesel::insert_into(tag_entities::table)
        .values(TagEntity {
            tag_id: ids.tools.clone(),
            entity_id: ids.drill.clone(),
        })
        .execute(conn)
        .expect("tag Drill with Tools");

    diesel::insert_into(attachments::table)
        .values(&vec![
            attachment(
                &ids.photo,
                &ids.drill,
                AttachmentKind::Photo,
                true,
                &"aa".repeat(32),
                3,
                12,
            ),
            attachment(
                &ids.manual,
                &ids.drill,
                AttachmentKind::Manual,
                false,
                &"bb".repeat(32),
                5,
                13,
            ),
        ])
        .execute(conn)
        .expect("insert sample attachments");
    diesel::insert_into(thumbnails::table)
        .values(Thumbnail {
            attachment_id: ids.photo.clone(),
            size: 500,
            mime_type: "image/webp".to_owned(),
            width: 4,
            height: 3,
            data: vec![0; 8],
            created_at: at(14),
        })
        .execute(conn)
        .expect("insert sample thumbnail");

    diesel::insert_into(entity_fields::table)
        .values(EntityField {
            id: "f-drill-voltage".to_owned(),
            entity_id: ids.drill.clone(),
            name: "Voltage".to_owned(),
            description: None,
            kind: FieldKind::Number,
            text_value: None,
            number_value: Some(18),
            boolean_value: false,
            time_value: None,
            created_at: at(15),
            updated_at: at(15),
        })
        .execute(conn)
        .expect("insert sample custom field");

    ids
}
