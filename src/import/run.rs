//! Upserts a Homebox backup in dependency order inside one transaction.
//!
//! Every table is upserted by primary key, so re-running an import refreshes
//! rows in place and never deletes a row the backup holds. Links (parents,
//! tags, templates) are written in a second pass once both ends exist; a link
//! whose target is not in the backup, or an entity parent link that would
//! form a cycle, is dropped with a warning rather than failing the import.
//! When an attachment's bytes changed since the last import, its old original
//! and that original's thumbnails are removed once the import commits, unless
//! something else still shares them.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::path::Path;
use std::{fs, iter};

use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};
use diesel::dsl::exists;
use diesel::prelude::*;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use super::report::{ImportReport, TableCounts};
use super::source::{Source, is_plain_attachment_id};
use super::tables::{
    AttachmentRow, EntityFieldRow, EntityRow, EntityTemplateRow, EntityTypeRow, Manifest,
    TagEntityRow, TagRow, TemplateFieldRow, parse_date, parse_timestamp,
};
use crate::asset_id::AssetId;
use crate::db::{ITEM_TYPE_ID, LOCATION_TYPE_ID};
use crate::kinds::{AttachmentKind, FieldKind};
use crate::models::{
    Attachment, Entity, EntityField, EntityTemplate, EntityType, Tag, TagEntity, TemplateField,
    Thumbnail,
};
use crate::money::Cents;
use crate::schema::{
    attachments, entities, entity_fields, entity_templates, entity_types, tag_entities, tags,
    template_fields, thumbnails,
};
use crate::svc::{attachment, tag};

const SUPPORTED_SCHEMA_VERSION: i64 = 1;
/// Homebox's stored thumbnails are 500px renditions; we keep them at that size.
const HOMEBOX_THUMBNAIL_SIZE: i32 = 500;
/// The Homebox attachment `type` of thumbnail rows, which are not attachments here.
const THUMBNAIL_KIND: &str = "thumbnail";

/// Inserts `$row` into `$table`, or overwrites its non-key columns when the
/// primary key `$key` already exists, and counts the outcome under the table's
/// name. A macro because diesel's upsert bounds do not abstract over tables.
macro_rules! upsert {
    ($conn:expr, $report:expr, $table:ident, $key:expr, $row:expr) => {{
        let row = $row;
        let existed: bool =
            diesel::select(exists($table::table.find($key))).get_result(&mut *$conn)?;
        diesel::insert_into($table::table)
            .values(&row)
            .on_conflict($table::table.primary_key())
            .do_update()
            .set(&row)
            .execute(&mut *$conn)
            .with_context(|| format!("upserting {} {:?}", stringify!($table), $key))?;
        bump($report.counts(stringify!($table)), existed);
    }};
}

/// Imports everything `source` holds, copying attachment blobs into
/// `data_dir/originals/<sha256>`. The database side is all-or-nothing.
pub fn import_backup(
    conn: &mut SqliteConnection,
    source: &mut dyn Source,
    data_dir: &Path,
) -> Result<ImportReport> {
    let manifest: Manifest = read_table(source, "manifest")?
        .context("manifest.json is missing; is this a Homebox backup?")?;
    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        bail!(
            "unsupported backup: schemaVersion {} (this importer understands schemaVersion {SUPPORTED_SCHEMA_VERSION})",
            manifest.schema_version
        );
    }

    // Read every table first so a malformed file fails before any write.
    let backup = Tables {
        types: read_rows(source, "entity_types")?,
        templates: read_rows(source, "entity_templates")?,
        template_fields: read_rows(source, "template_fields")?,
        tags: read_rows(source, "tags")?,
        entities: read_rows(source, "entities")?,
        fields: read_rows(source, "entity_fields")?,
        tag_links: read_rows(source, "tag_entities")?,
        attachments: read_rows(source, "attachments")?,
    };
    let maintenance: Vec<serde_json::Value> = read_rows(source, "maintenance_entries")?;
    let notifiers: Vec<serde_json::Value> = read_rows(source, "notifiers")?;

    let originals_dir = data_dir.join("originals");
    fs::create_dir_all(&originals_dir)
        .with_context(|| format!("creating {}", originals_dir.display()))?;

    let mut report = ImportReport::default();
    for (count, what) in [
        (maintenance.len(), "maintenance entries"),
        (notifiers.len(), "notifiers"),
    ] {
        if count > 0 {
            report.warn(format!(
                "{count} {what} were not imported (not supported in v1)"
            ));
        }
    }

    // Originals are written inside the transaction but are content-addressed,
    // so a rollback at worst leaves an unreferenced file that the next run reuses.
    let replaced = conn.transaction::<_, anyhow::Error, _>(|conn| {
        let type_ids = import_entity_types(conn, &backup.types, &mut report)?;
        let template_ids = import_templates(
            conn,
            &backup.templates,
            &backup.template_fields,
            &mut report,
        )?;
        patch_default_templates(conn, &backup.types, &type_ids, &template_ids, &mut report)?;
        let tag_ids = import_tags(conn, &backup.tags, &mut report)?;
        let entity_ids = import_entities(conn, &backup.entities, &type_ids, &mut report)?;
        patch_template_locations(conn, &backup.templates, &entity_ids, &mut report)?;
        import_entity_fields(conn, &backup.fields, &entity_ids, &mut report)?;
        import_tag_links(conn, &backup.tag_links, &tag_ids, &entity_ids, &mut report)?;
        import_attachments(
            conn,
            source,
            &backup.attachments,
            &entity_ids,
            &originals_dir,
            &mut report,
        )
    })?;
    // Only once the new hashes are committed can the old ones be unshared.
    for sha256 in replaced {
        attachment::remove_original(conn, data_dir, &sha256);
    }
    Ok(report)
}

/// The parsed tables of one backup; absent tables are empty.
struct Tables {
    types: Vec<EntityTypeRow>,
    templates: Vec<EntityTemplateRow>,
    template_fields: Vec<TemplateFieldRow>,
    tags: Vec<TagRow>,
    entities: Vec<EntityRow>,
    fields: Vec<EntityFieldRow>,
    tag_links: Vec<TagEntityRow>,
    attachments: Vec<AttachmentRow>,
}

fn read_table<T: DeserializeOwned>(source: &mut dyn Source, name: &str) -> Result<Option<T>> {
    source
        .read_table(name)?
        .map(|bytes| serde_json::from_slice(&bytes).with_context(|| format!("parsing {name}.json")))
        .transpose()
}

/// A table that may be absent from the backup, which reads as no rows.
fn read_rows<T: DeserializeOwned>(source: &mut dyn Source, name: &str) -> Result<Vec<T>> {
    Ok(read_table(source, name)?.unwrap_or_default())
}

fn bump(counts: &mut TableCounts, existed: bool) {
    if existed {
        counts.updated += 1;
    } else {
        counts.inserted += 1;
    }
}

/// A row's `(created_at, updated_at)`, with the row named in any parse error.
fn stamps(
    created: &str,
    updated: &str,
    row: impl Display,
) -> Result<(NaiveDateTime, NaiveDateTime)> {
    let parse = |s: &str| parse_timestamp(s).with_context(|| format!("{row}: bad timestamp {s:?}"));
    Ok((parse(created)?, parse(updated)?))
}

fn optional_date(value: Option<&str>, row: impl Display) -> Result<Option<NaiveDate>> {
    value
        .map(|s| parse_date(s).with_context(|| format!("{row}: bad date {s:?}")))
        .transpose()
}

fn optional_timestamp(value: Option<&str>, row: impl Display) -> Result<Option<NaiveDateTime>> {
    value
        .map(|s| parse_timestamp(s).with_context(|| format!("{row}: bad timestamp {s:?}")))
        .transpose()
}

/// Returns Homebox type id → our type id (built-ins remapped by name).
fn import_entity_types(
    conn: &mut SqliteConnection,
    rows: &[EntityTypeRow],
    report: &mut ImportReport,
) -> Result<HashMap<String, String>> {
    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let builtin = match row.name.as_str() {
            "global.location" => Some(LOCATION_TYPE_ID),
            "global.item" => Some(ITEM_TYPE_ID),
            _ => None,
        };
        if let Some(ours) = builtin {
            map.insert(row.id.clone(), ours.to_owned());
            report.counts("entity_types").skipped += 1;
            continue;
        }
        let (created_at, updated_at) = stamps(
            &row.created_at,
            &row.updated_at,
            format!("entity type {}", row.id),
        )?;
        upsert!(
            conn,
            report,
            entity_types,
            &row.id,
            EntityType {
                id: row.id.clone(),
                name: row.name.clone(),
                description: row.description.clone(),
                icon: row.icon.clone(),
                is_location: row.is_location,
                // Set by `patch_default_templates` once templates exist.
                default_template_id: None,
                created_at,
                updated_at,
            }
        );
        map.insert(row.id.clone(), row.id.clone());
    }
    Ok(map)
}

/// Upserts templates (without their location, which needs entities) and their
/// fields; returns the imported template ids.
fn import_templates(
    conn: &mut SqliteConnection,
    templates: &[EntityTemplateRow],
    fields: &[TemplateFieldRow],
    report: &mut ImportReport,
) -> Result<HashSet<String>> {
    for row in templates {
        let what = format!("template {}", row.id);
        let (created_at, updated_at) = stamps(&row.created_at, &row.updated_at, &what)?;
        let default_tag_ids = row
            .default_tag_ids
            .as_ref()
            .filter(|v| !v.is_null())
            .map(serde_json::to_string)
            .transpose()
            .with_context(|| format!("{what}: encoding default_tag_ids"))?;
        upsert!(
            conn,
            report,
            entity_templates,
            &row.id,
            EntityTemplate {
                id: row.id.clone(),
                name: row.name.clone(),
                description: row.description.clone(),
                notes: row.notes.clone(),
                default_quantity: row.default_quantity,
                default_insured: row.default_insured,
                default_name: row.default_name.clone(),
                default_description: row.default_description.clone(),
                default_manufacturer: row.default_manufacturer.clone(),
                default_model_number: row.default_model_number.clone(),
                default_lifetime_warranty: row.default_lifetime_warranty,
                default_warranty_details: row.default_warranty_details.clone(),
                include_warranty_fields: row.include_warranty_fields,
                include_purchase_fields: row.include_purchase_fields,
                include_sold_fields: row.include_sold_fields,
                default_tag_ids,
                // Set by `patch_template_locations` once entities exist.
                location_id: None,
                created_at,
                updated_at,
            }
        );
    }
    let template_ids: HashSet<String> = templates.iter().map(|t| t.id.clone()).collect();

    for row in fields {
        let what = format!("template field {}", row.id);
        if !template_ids.contains(&row.entity_template_fields) {
            report.warn(format!(
                "{what} belongs to missing template {}; skipped",
                row.entity_template_fields
            ));
            report.counts("template_fields").skipped += 1;
            continue;
        }
        let Some(kind) = field_kind(&row.kind, &what, "template_fields", report) else {
            continue;
        };
        let (created_at, updated_at) = stamps(&row.created_at, &row.updated_at, &what)?;
        upsert!(
            conn,
            report,
            template_fields,
            &row.id,
            TemplateField {
                id: row.id.clone(),
                template_id: row.entity_template_fields.clone(),
                name: row.name.clone(),
                description: row.description.clone(),
                kind,
                text_value: row.text_value.clone(),
                number_value: row.number_value,
                boolean_value: row.boolean_value,
                time_value: optional_timestamp(row.time_value.as_deref(), &what)?,
                created_at,
                updated_at,
            }
        );
    }
    Ok(template_ids)
}

/// Parses a custom field's `type`; an unknown one is warned about and counted
/// as skipped under `table`.
fn field_kind(
    raw: &str,
    what: &str,
    table: &'static str,
    report: &mut ImportReport,
) -> Option<FieldKind> {
    match raw.parse() {
        Ok(kind) => Some(kind),
        Err(e) => {
            report.warn(format!("{what}: {e}; skipped"));
            report.counts(table).skipped += 1;
            None
        }
    }
}

fn patch_default_templates(
    conn: &mut SqliteConnection,
    types: &[EntityTypeRow],
    type_ids: &HashMap<String, String>,
    template_ids: &HashSet<String>,
    report: &mut ImportReport,
) -> Result<()> {
    for row in types {
        let Some(template) = &row.entity_type_default_template else {
            continue;
        };
        if !template_ids.contains(template) {
            report.warn(format!(
                "entity type {} references missing default template {template}; left unset",
                row.id
            ));
            continue;
        }
        let ours = type_ids
            .get(&row.id)
            .with_context(|| format!("entity type {} was not imported", row.id))?;
        diesel::update(entity_types::table.find(ours))
            .set(entity_types::default_template_id.eq(template))
            .execute(conn)
            .with_context(|| format!("setting the default template of entity type {ours}"))?;
    }
    Ok(())
}

/// Returns the ids of every imported tag.
fn import_tags(
    conn: &mut SqliteConnection,
    rows: &[TagRow],
    report: &mut ImportReport,
) -> Result<HashSet<String>> {
    for row in rows {
        let (created_at, updated_at) =
            stamps(&row.created_at, &row.updated_at, format!("tag {}", row.id))?;
        let color = tag::optional_color(row.color.clone()).unwrap_or_else(|e| {
            report.warn(format!(
                "tag {} ({}): {e}, got {:?}; imported without a colour",
                row.id,
                row.name,
                row.color.as_deref().unwrap_or_default()
            ));
            None
        });
        upsert!(
            conn,
            report,
            tags,
            &row.id,
            Tag {
                id: row.id.clone(),
                name: row.name.clone(),
                description: row.description.clone(),
                color,
                icon: row.icon.clone(),
                // Set below once every tag exists.
                parent_id: None,
                created_at,
                updated_at,
            }
        );
    }
    let tag_ids: HashSet<String> = rows.iter().map(|t| t.id.clone()).collect();
    for row in rows {
        let Some(parent) = &row.tag_children else {
            continue;
        };
        if !tag_ids.contains(parent) {
            report.warn(format!(
                "tag {} references missing parent {parent}; imported as a root",
                row.id
            ));
            continue;
        }
        diesel::update(tags::table.find(&row.id))
            .set(tags::parent_id.eq(parent))
            .execute(conn)
            .with_context(|| format!("setting the parent of tag {}", row.id))?;
    }
    Ok(tag_ids)
}

/// Returns the ids of every imported entity.
fn import_entities(
    conn: &mut SqliteConnection,
    rows: &[EntityRow],
    type_ids: &HashMap<String, String>,
    report: &mut ImportReport,
) -> Result<HashSet<String>> {
    for row in rows {
        let what = format!("entity {}", row.id);
        let Some(entity_type_id) = type_ids.get(&row.entity_type_entities) else {
            bail!(
                "entity {} references unknown entity type {}",
                row.id,
                row.entity_type_entities
            );
        };
        let (created_at, updated_at) = stamps(&row.created_at, &row.updated_at, &what)?;
        upsert!(
            conn,
            report,
            entities,
            &row.id,
            Entity {
                id: row.id.clone(),
                name: row.name.clone(),
                description: row.description.clone(),
                entity_type_id: entity_type_id.clone(),
                // Set below once every entity exists.
                parent_id: None,
                archived: row.archived,
                asset_id: AssetId(row.asset_id),
                import_ref: row.import_ref.clone(),
                notes: row.notes.clone(),
                quantity: row.quantity,
                insured: row.insured,
                serial_number: row.serial_number.clone(),
                model_number: row.model_number.clone(),
                manufacturer: row.manufacturer.clone(),
                lifetime_warranty: row.lifetime_warranty,
                warranty_expires: optional_date(row.warranty_expires.as_deref(), &what)?,
                warranty_details: row.warranty_details.clone(),
                purchase_date: optional_date(row.purchase_date.as_deref(), &what)?,
                purchase_from: row.purchase_from.clone(),
                purchase_price_cents: Cents::from_major_f64(row.purchase_price),
                sold_date: optional_date(row.sold_date.as_deref(), &what)?,
                sold_to: row.sold_to.clone(),
                sold_price_cents: Cents::from_major_f64(row.sold_price),
                sold_notes: row.sold_notes.clone(),
                sync_child_entity_locations: row.sync_child_entity_locations,
                created_at,
                updated_at,
            }
        );
    }
    let entity_ids: HashSet<String> = rows.iter().map(|e| e.id.clone()).collect();
    // Links accepted so far, child → parent, so one that would close a loop
    // (which would make every ancestor walk spin forever) can be refused.
    let mut parents: HashMap<&str, &str> = HashMap::new();
    for row in rows {
        // `entity_children` holds the PARENT id despite its name.
        let Some(parent) = row.entity_children.as_deref() else {
            continue;
        };
        let id = row.id.as_str();
        let refusal = if !entity_ids.contains(parent) {
            Some(format!("references missing parent {parent}"))
        } else if parent == id {
            Some("is its own parent".to_owned())
        } else if reaches(&parents, parent, id) {
            Some(format!("would close a parent cycle through {parent}"))
        } else {
            None
        };
        if let Some(reason) = refusal {
            report.warn(format!("entity {id} {reason}; imported as a root"));
            continue;
        }
        parents.insert(id, parent);
        diesel::update(entities::table.find(id))
            .set(entities::parent_id.eq(parent))
            .execute(conn)
            .with_context(|| format!("setting the parent of entity {id}"))?;
    }
    Ok(entity_ids)
}

/// Whether walking up `parents` from `start` (inclusive) reaches `target`. The
/// map only ever gains links that pass this check, so it stays acyclic and the
/// walk always ends.
fn reaches(parents: &HashMap<&str, &str>, start: &str, target: &str) -> bool {
    iter::successors(Some(start), |id| parents.get(id).copied()).any(|id| id == target)
}

fn patch_template_locations(
    conn: &mut SqliteConnection,
    templates: &[EntityTemplateRow],
    entity_ids: &HashSet<String>,
    report: &mut ImportReport,
) -> Result<()> {
    for row in templates {
        let Some(location) = &row.entity_template_location else {
            continue;
        };
        if !entity_ids.contains(location) {
            report.warn(format!(
                "template {} references missing location {location}; left unset",
                row.id
            ));
            continue;
        }
        diesel::update(entity_templates::table.find(&row.id))
            .set(entity_templates::location_id.eq(location))
            .execute(conn)
            .with_context(|| format!("setting the location of template {}", row.id))?;
    }
    Ok(())
}

fn import_entity_fields(
    conn: &mut SqliteConnection,
    rows: &[EntityFieldRow],
    entity_ids: &HashSet<String>,
    report: &mut ImportReport,
) -> Result<()> {
    for row in rows {
        let what = format!("entity field {}", row.id);
        if !entity_ids.contains(&row.entity_fields) {
            report.warn(format!(
                "{what} belongs to missing entity {}; skipped",
                row.entity_fields
            ));
            report.counts("entity_fields").skipped += 1;
            continue;
        }
        let Some(kind) = field_kind(&row.kind, &what, "entity_fields", report) else {
            continue;
        };
        let (created_at, updated_at) = stamps(&row.created_at, &row.updated_at, &what)?;
        upsert!(
            conn,
            report,
            entity_fields,
            &row.id,
            EntityField {
                id: row.id.clone(),
                entity_id: row.entity_fields.clone(),
                name: row.name.clone(),
                description: row.description.clone(),
                kind,
                text_value: row.text_value.clone(),
                number_value: row.number_value,
                boolean_value: row.boolean_value,
                time_value: optional_timestamp(row.time_value.as_deref(), &what)?,
                created_at,
                updated_at,
            }
        );
    }
    Ok(())
}

fn import_tag_links(
    conn: &mut SqliteConnection,
    rows: &[TagEntityRow],
    tag_ids: &HashSet<String>,
    entity_ids: &HashSet<String>,
    report: &mut ImportReport,
) -> Result<()> {
    for row in rows {
        if !tag_ids.contains(&row.tag_id) || !entity_ids.contains(&row.entity_id) {
            report.warn(format!(
                "tag link {} → {} references a missing tag or entity; skipped",
                row.tag_id, row.entity_id
            ));
            report.counts("tag_entities").skipped += 1;
            continue;
        }
        let written = diesel::insert_into(tag_entities::table)
            .values(TagEntity {
                tag_id: row.tag_id.clone(),
                entity_id: row.entity_id.clone(),
            })
            .on_conflict_do_nothing()
            .execute(conn)
            .with_context(|| format!("linking tag {} to {}", row.tag_id, row.entity_id))?;
        let counts = report.counts("tag_entities");
        if written == 0 {
            counts.skipped += 1;
        } else {
            counts.inserted += 1;
        }
    }
    Ok(())
}

/// Upserts the attachments and their Homebox thumbnails. Returns the hashes
/// the upserts replaced, whose originals may now be unreferenced.
fn import_attachments(
    conn: &mut SqliteConnection,
    source: &mut dyn Source,
    rows: &[AttachmentRow],
    entity_ids: &HashSet<String>,
    originals_dir: &Path,
    report: &mut ImportReport,
) -> Result<Vec<String>> {
    let (thumbs, files): (Vec<&AttachmentRow>, Vec<&AttachmentRow>) =
        rows.iter().partition(|r| r.kind == THUMBNAIL_KIND);
    let thumbs: HashMap<&str, &AttachmentRow> =
        thumbs.into_iter().map(|t| (t.id.as_str(), t)).collect();
    let mut replaced = Vec::new();

    for row in files {
        let what = format!("attachment {}", row.id);
        if !is_plain_attachment_id(&row.id) {
            report.warn(format!("{what}: id is not a plain file name; skipped"));
            report.counts("attachments").skipped += 1;
            continue;
        }
        let Some(entity_id) = row
            .entity_attachments
            .as_ref()
            .filter(|e| entity_ids.contains(*e))
        else {
            report.warn(format!(
                "{what} belongs to missing entity {}; skipped",
                row.entity_attachments.as_deref().unwrap_or("(none)")
            ));
            report.counts("attachments").skipped += 1;
            continue;
        };
        let kind: AttachmentKind = match row.kind.parse() {
            Ok(kind) => kind,
            Err(e) => {
                report.warn(format!("{what}: {e}; skipped"));
                report.counts("attachments").skipped += 1;
                continue;
            }
        };
        let Some(bytes) = source.read_attachment(&row.id)? else {
            report.warn(format!("{what} has no blob in the backup; skipped"));
            report.counts("attachments").skipped += 1;
            continue;
        };
        let sha256 = hex::encode(Sha256::digest(&bytes));
        if store_original(originals_dir, &sha256, &bytes)? {
            report.originals_written += 1;
        }
        replaced.extend(replaced_sha256(conn, &row.id, &sha256)?);
        let (created_at, updated_at) = stamps(&row.created_at, &row.updated_at, &what)?;
        upsert!(
            conn,
            report,
            attachments,
            &row.id,
            Attachment {
                id: row.id.clone(),
                entity_id: entity_id.clone(),
                kind,
                is_primary: row.primary,
                title: row.title.clone(),
                mime_type: row.mime_type.clone(),
                sha256: sha256.clone(),
                size_bytes: i64::try_from(bytes.len())
                    .with_context(|| format!("{what} is too large"))?,
                created_at,
                updated_at,
            }
        );

        if let Some(thumb_id) = &row.attachment_thumbnail {
            import_thumbnail(
                conn,
                source,
                &row.id,
                &sha256,
                thumbs.get(thumb_id.as_str()).copied(),
                report,
            )?;
        }
    }
    Ok(replaced)
}

/// The hash `attachment_id` is stored with, when it exists with bytes other
/// than `sha256`: the upsert about to run replaces it.
fn replaced_sha256(
    conn: &mut SqliteConnection,
    attachment_id: &str,
    sha256: &str,
) -> Result<Option<String>> {
    let previous: Option<String> = attachments::table
        .find(attachment_id)
        .select(attachments::sha256)
        .first(conn)
        .optional()
        .with_context(|| format!("reading the digest of attachment {attachment_id}"))?;
    Ok(previous.filter(|old| old != sha256))
}

/// Stores Homebox's pre-rendered thumbnail of `attachment_id`, whose bytes
/// hash to `sha256`, as that original's 500px variant. A missing or
/// undecodable thumbnail is only a warning: the original is imported and a
/// thumbnail can be regenerated from it.
fn import_thumbnail(
    conn: &mut SqliteConnection,
    source: &mut dyn Source,
    attachment_id: &str,
    sha256: &str,
    thumb: Option<&AttachmentRow>,
    report: &mut ImportReport,
) -> Result<()> {
    let Some(thumb) = thumb else {
        report.warn(format!(
            "attachment {attachment_id} references a missing thumbnail row; no thumbnail stored"
        ));
        return Ok(());
    };
    if !is_plain_attachment_id(&thumb.id) {
        report.warn(format!(
            "thumbnail {} of attachment {attachment_id}: id is not a plain file name; no thumbnail stored",
            thumb.id
        ));
        return Ok(());
    }
    let Some(data) = source.read_attachment(&thumb.id)? else {
        report.warn(format!(
            "thumbnail {} of attachment {attachment_id} has no blob in the backup; no thumbnail stored",
            thumb.id
        ));
        return Ok(());
    };
    let dimensions = image::load_from_memory(&data)
        .map_err(anyhow::Error::from)
        .and_then(|img| Ok((i32::try_from(img.width())?, i32::try_from(img.height())?)));
    let (width, height) = match dimensions {
        Ok(dims) => dims,
        Err(e) => {
            report.warn(format!(
                "thumbnail {} of attachment {attachment_id} could not be decoded ({e}); no thumbnail stored",
                thumb.id
            ));
            return Ok(());
        }
    };
    let (created_at, _) = stamps(
        &thumb.created_at,
        &thumb.updated_at,
        format!("thumbnail {}", thumb.id),
    )?;
    upsert!(
        conn,
        report,
        thumbnails,
        (sha256, HOMEBOX_THUMBNAIL_SIZE),
        Thumbnail {
            sha256: sha256.to_owned(),
            size: HOMEBOX_THUMBNAIL_SIZE,
            mime_type: thumb.mime_type.clone(),
            width,
            height,
            data,
            created_at,
        }
    );
    Ok(())
}

/// Writes `bytes` to `dir/<sha256>` unless it is already there; returns whether
/// it wrote. Goes through a temp file and a rename so an interrupted import
/// never leaves a truncated file under a hash name that later runs would trust.
fn store_original(dir: &Path, sha256: &str, bytes: &[u8]) -> Result<bool> {
    let path = dir.join(sha256);
    if path.exists() {
        return Ok(false);
    }
    let tmp = dir.join(format!("{sha256}.partial"));
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("moving into {}", path.display()))?;
    Ok(true)
}
