//! A synthetic Homebox backup mirroring the real export's shapes: three types,
//! four entities in a two-level tree, two nested tags, one custom field, a
//! photo with its WebP thumbnail, a PDF manual, one template with one field,
//! and one (unsupported) maintenance entry. Test-only code: failures panic.

pub mod openai_stub;

use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// The ids (and photo hash) of everything [`MiniBackup`] writes.
#[derive(Debug, Clone)]
pub struct MiniIds {
    pub garage: String,
    pub tote1: String,
    pub router: String,
    pub cable: String,
    pub iot: String,
    pub general: String,
    pub photo: String,
    pub thumb: String,
    pub manual: String,
    pub tote_type: String,
    pub template: String,
    pub template_field: String,
    pub jpeg_sha256: String,
}

/// Writes the mini backup as an exploded directory or as a zip.
pub struct MiniBackup;

impl MiniBackup {
    pub fn write_dir(dir: &Path) -> MiniIds {
        let (ids, entries) = build();
        for (name, bytes) in entries {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        ids
    }

    pub fn write_zip(path: &Path) -> MiniIds {
        Self::write_zip_with_prefix(path, "", false)
    }

    /// Like [`Self::write_zip`], but every entry lives under `prefix` (e.g.
    /// `"homebox-backup/"`), as when a user re-zips an exploded backup folder.
    /// With `macos_metadata`, the zip also holds the `__MACOSX/` resource-fork
    /// entries macOS Finder adds when it compresses a folder.
    pub fn write_zip_with_prefix(path: &Path, prefix: &str, macos_metadata: bool) -> MiniIds {
        let (ids, entries) = build();
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        if !prefix.is_empty() {
            zip.add_directory(prefix, SimpleFileOptions::default())
                .unwrap();
        }
        for (name, bytes) in entries {
            zip.start_file(format!("{prefix}{name}"), SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        if macos_metadata {
            zip.add_directory("__MACOSX/", SimpleFileOptions::default())
                .unwrap();
            zip.start_file("__MACOSX/._manifest.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"\0\x05\x16\x07 resource fork").unwrap();
        }
        zip.finish().unwrap();
        ids
    }
}

/// A 4×3 gradient, so the encoders have something non-trivial to compress.
fn picture() -> RgbImage {
    RgbImage::from_fn(4, 3, |x, y| Rgb([(x * 60) as u8, (y * 80) as u8, 128]))
}

/// A solid 4×3 image of `colour`, encoded as the JPEG and WebP blobs of the
/// photo and its thumbnail, for tests that change a blob between imports.
pub fn recoloured_photo(colour: [u8; 3]) -> (Vec<u8>, Vec<u8>) {
    let img = RgbImage::from_pixel(4, 3, Rgb(colour));
    (jpeg_bytes(&img), webp_bytes(&img))
}

fn jpeg_bytes(img: &RgbImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    JpegEncoder::new(&mut out)
        .write_image(img, img.width(), img.height(), ExtendedColorType::Rgb8)
        .unwrap();
    out.into_inner()
}

fn webp_bytes(img: &RgbImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    WebPEncoder::new_lossless(&mut out)
        .write_image(img, img.width(), img.height(), ExtendedColorType::Rgb8)
        .unwrap();
    out.into_inner()
}

fn stamp(second: u32) -> String {
    format!("2024-09-13T12:23:{second:02}.459464753Z")
}

fn entity(id: &str, name: &str, type_id: &str, parent: Option<&str>, second: u32) -> Value {
    json!({
        "id": id, "name": name, "description": "", "entity_type_entities": type_id,
        "entity_children": parent, "group_entities": "group-1", "archived": 0,
        "asset_id": 0, "import_ref": null, "notes": null, "quantity": 1, "insured": 0,
        "serial_number": null, "model_number": null, "manufacturer": null,
        "lifetime_warranty": 0, "warranty_expires": null, "warranty_details": null,
        "purchase_date": null, "purchase_from": null, "purchase_price": 0,
        "sold_date": null, "sold_to": null, "sold_price": 0, "sold_notes": null,
        "sync_child_entity_locations": 0,
        "created_at": stamp(second), "updated_at": stamp(second),
    })
}

fn to_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

/// Every file of the backup as `(path inside the backup, bytes)`.
fn build() -> (MiniIds, Vec<(String, Vec<u8>)>) {
    let img = picture();
    let jpeg = jpeg_bytes(&img);
    let webp = webp_bytes(&img);
    let pdf = b"%PDF-1.4 mini".to_vec();
    let jpeg_sha256 = hex::encode(Sha256::digest(&jpeg));
    let webp_sha256 = hex::encode(Sha256::digest(&webp));
    let pdf_sha256 = hex::encode(Sha256::digest(&pdf));

    let ids = MiniIds {
        garage: "ent-garage".to_owned(),
        tote1: "ent-tote-1".to_owned(),
        router: "ent-router".to_owned(),
        cable: "ent-cable".to_owned(),
        iot: "tag-iot".to_owned(),
        general: "tag-general".to_owned(),
        photo: "att-photo".to_owned(),
        thumb: "att-thumb".to_owned(),
        manual: "att-manual".to_owned(),
        tote_type: "0192f0c4-7d1e-7a3b-9c4d-5e6f7a8b9c0d".to_owned(),
        template: "tpl-tote".to_owned(),
        template_field: "tpl-field-colour".to_owned(),
        jpeg_sha256: jpeg_sha256.clone(),
    };

    let manifest = json!({
        "schemaVersion": 1,
        "exportedAt": "2026-09-25T20:40:02.426625699Z",
        "groupId": "group-1",
        "counts": {
            "attachments": 3, "entities": 4, "entity_fields": 1, "entity_templates": 1,
            "entity_types": 3, "maintenance_entries": 1, "notifiers": 0,
            "tag_entities": 1, "tags": 2, "template_fields": 1,
        },
    });
    let entity_types = json!([
        {
            "id": "type-location", "name": "global.location", "description": "",
            "icon": null, "is_location": 1, "entity_type_default_template": null,
            "group_entity_types": "group-1",
            "created_at": "2026-08-27T16:00:08Z", "updated_at": "2026-08-27T16:00:08Z",
        },
        {
            "id": "type-item", "name": "global.item", "description": "",
            "icon": null, "is_location": 0, "entity_type_default_template": null,
            "group_entity_types": "group-1",
            "created_at": "2026-08-27T16:00:08Z", "updated_at": "2026-08-27T16:00:08Z",
        },
        {
            "id": ids.tote_type, "name": "Tote", "description": "A plastic storage tote",
            "icon": "mdi-package", "is_location": 1,
            "entity_type_default_template": ids.template, "group_entity_types": "group-1",
            "created_at": "2026-08-27T16:00:09Z", "updated_at": "2026-08-27T16:00:09Z",
        },
    ]);
    let entity_templates = json!([{
        "id": ids.template, "name": "Tote contents", "description": "", "notes": null,
        "default_quantity": 1, "default_insured": 0, "default_name": "Tote item",
        "default_description": null, "default_manufacturer": null,
        "default_model_number": null, "default_lifetime_warranty": 0,
        "default_warranty_details": null, "include_warranty_fields": 0,
        "include_purchase_fields": 1, "include_sold_fields": 0,
        "default_tag_ids": [ids.iot], "entity_template_location": ids.garage,
        "created_at": stamp(1), "updated_at": stamp(1),
    }]);
    let template_fields = json!([{
        "id": ids.template_field, "entity_template_fields": ids.template,
        "name": "Colour", "description": "", "type": "text", "text_value": "clear",
        "number_value": null, "boolean_value": 0, "time_value": null,
        "created_at": stamp(1), "updated_at": stamp(1),
    }]);
    let tags = json!([
        {
            "id": ids.iot, "name": "IOT", "description": "", "color": "", "icon": null,
            "tag_children": null, "group_tags": "group-1",
            "created_at": stamp(2), "updated_at": stamp(2),
        },
        {
            "id": ids.general, "name": "General", "description": "", "color": "#ff0000",
            "icon": null, "tag_children": ids.iot, "group_tags": "group-1",
            "created_at": stamp(3), "updated_at": stamp(3),
        },
    ]);

    let mut router = entity(&ids.router, "Router", "type-item", Some(&ids.garage), 6);
    router["purchase_price"] = json!(1099.99);
    router["asset_id"] = json!(7);
    router["purchase_date"] = json!("2021-10-23T00:00:00Z");
    router["insured"] = json!(1);
    let mut cable = entity(&ids.cable, "Cable", "type-item", Some(&ids.tote1), 7);
    cable["purchase_price"] = json!(4.5);
    cable["quantity"] = json!(3);
    let mut garage = entity(&ids.garage, "Garage", "type-location", None, 4);
    garage["asset_id"] = json!(2);
    let entities = json!([
        garage,
        entity(&ids.tote1, "Tote 1", &ids.tote_type, Some(&ids.garage), 5),
        router,
        cable,
    ]);

    let entity_fields = json!([{
        "id": "field-router-model", "entity_fields": ids.router, "name": "Model",
        "description": "", "type": "text", "text_value": "AX1800",
        "number_value": null, "boolean_value": 0, "time_value": null,
        "created_at": stamp(8), "updated_at": stamp(8),
    }]);
    let tag_entities = json!([{ "tag_id": ids.iot, "entity_id": ids.router }]);
    let attachments = json!([
        {
            "id": ids.photo, "entity_attachments": ids.router, "type": "photo", "primary": 1,
            "title": "image.jpg", "mime_type": "image/jpeg",
            "path": format!("group-1/documents/{jpeg_sha256}"),
            "attachment_thumbnail": ids.thumb,
            "created_at": stamp(9), "updated_at": stamp(9),
        },
        {
            "id": ids.thumb, "entity_attachments": null, "type": "thumbnail", "primary": 0,
            "title": "image.jpg-thumb", "mime_type": "image/webp",
            "path": format!("group-1/documents/{webp_sha256}"),
            "attachment_thumbnail": null,
            "created_at": stamp(9), "updated_at": stamp(9),
        },
        {
            "id": ids.manual, "entity_attachments": ids.router, "type": "manual",
            "primary": 0, "title": "manual.pdf", "mime_type": "application/pdf",
            "path": format!("group-1/documents/{pdf_sha256}"),
            "attachment_thumbnail": null,
            "created_at": stamp(10), "updated_at": stamp(10),
        },
    ]);
    let maintenance_entries = json!([{
        "id": "maint-1", "entity_id": ids.router, "name": "Firmware update",
        "date": "2025-01-01T00:00:00Z", "cost": 0,
    }]);

    let entries = vec![
        ("manifest.json".to_owned(), to_bytes(&manifest)),
        ("entity_types.json".to_owned(), to_bytes(&entity_types)),
        (
            "entity_templates.json".to_owned(),
            to_bytes(&entity_templates),
        ),
        (
            "template_fields.json".to_owned(),
            to_bytes(&template_fields),
        ),
        ("tags.json".to_owned(), to_bytes(&tags)),
        ("entities.json".to_owned(), to_bytes(&entities)),
        ("entity_fields.json".to_owned(), to_bytes(&entity_fields)),
        ("tag_entities.json".to_owned(), to_bytes(&tag_entities)),
        ("attachments.json".to_owned(), to_bytes(&attachments)),
        (
            "maintenance_entries.json".to_owned(),
            to_bytes(&maintenance_entries),
        ),
        ("notifiers.json".to_owned(), to_bytes(&json!([]))),
        (format!("attachments/{}", ids.photo), jpeg),
        (format!("attachments/{}", ids.thumb), webp),
        (format!("attachments/{}", ids.manual), pdf),
    ];
    (ids, entries)
}
