mod support;

use std::fs;
use std::path::Path;

use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use home_tracker::asset_id::AssetId;
use home_tracker::db::{ITEM_TYPE_ID, LOCATION_TYPE_ID, TestDb};
use home_tracker::import::report::{ImportReport, TableCounts};
use home_tracker::import::run::import_backup;
use home_tracker::import::source::{self, DirSource, Source};
use home_tracker::kinds::{AttachmentKind, FieldKind};
use home_tracker::models::{Attachment, Entity, EntityTemplate, EntityType, TemplateField};
use home_tracker::money::Cents;
use home_tracker::schema::{
    attachments, entities, entity_fields, entity_templates, entity_types, tag_entities, tags,
    template_fields, thumbnails,
};
use home_tracker::svc;
use serde_json::Value;
use support::{MiniBackup, MiniIds};
use tempfile::TempDir;

/// A fresh database, a data dir, and the mini backup exploded into a directory.
struct Harness {
    db: TestDb,
    data: TempDir,
    backup: TempDir,
    ids: MiniIds,
}

impl Harness {
    fn new() -> Self {
        let backup = tempfile::tempdir().unwrap();
        let ids = MiniBackup::write_dir(backup.path());
        Self {
            db: TestDb::new(),
            data: tempfile::tempdir().unwrap(),
            backup,
            ids,
        }
    }

    fn import(&self) -> anyhow::Result<ImportReport> {
        let mut conn = self.db.pool.get().unwrap();
        let mut source = DirSource::new(self.backup.path());
        import_backup(&mut conn, &mut source, self.data.path())
    }

    fn conn(&self) -> PooledConnection<ConnectionManager<SqliteConnection>> {
        self.db.pool.get().unwrap()
    }

    fn entity(&self, id: &str) -> Entity {
        svc::entity::get(&mut self.conn(), id).unwrap().unwrap()
    }

    /// Rewrites `<name>.json` in the backup directory through `edit`.
    fn edit_table(&self, name: &str, edit: impl FnOnce(&mut Value)) {
        let path = self.backup.path().join(format!("{name}.json"));
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        edit(&mut value);
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    /// The row of `rows` (a JSON array) whose `id` is `id`.
    fn row<'a>(rows: &'a mut Value, id: &str) -> &'a mut Value {
        rows.as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == id)
            .unwrap()
    }
}

fn counts(report: &ImportReport, table: &str) -> TableCounts {
    report.tables.get(table).cloned().unwrap_or_default()
}

fn inserted(n: u64) -> TableCounts {
    TableCounts {
        inserted: n,
        ..TableCounts::default()
    }
}

fn row_counts(conn: &mut SqliteConnection) -> Vec<i64> {
    vec![
        entity_types::table.count().get_result(conn).unwrap(),
        entity_templates::table.count().get_result(conn).unwrap(),
        template_fields::table.count().get_result(conn).unwrap(),
        tags::table.count().get_result(conn).unwrap(),
        entities::table.count().get_result(conn).unwrap(),
        entity_fields::table.count().get_result(conn).unwrap(),
        tag_entities::table.count().get_result(conn).unwrap(),
        attachments::table.count().get_result(conn).unwrap(),
        thumbnails::table.count().get_result(conn).unwrap(),
    ]
}

/// The sorted file names directly inside `dir`.
fn dir_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn all_entities(conn: &mut SqliteConnection) -> Vec<Entity> {
    entities::table
        .order(entities::id)
        .select(Entity::as_select())
        .load(conn)
        .unwrap()
}

fn all_attachments(conn: &mut SqliteConnection) -> Vec<Attachment> {
    attachments::table
        .order(attachments::id)
        .select(Attachment::as_select())
        .load(conn)
        .unwrap()
}

#[test]
fn imports_the_mini_backup_from_a_directory() {
    let h = Harness::new();
    let report = h.import().unwrap();
    assert_eq!(
        counts(&report, "entity_types"),
        TableCounts {
            inserted: 1,
            updated: 0,
            skipped: 2
        }
    );
    assert_eq!(counts(&report, "entities"), inserted(4));
    assert_eq!(counts(&report, "tags"), inserted(2));
    assert_eq!(counts(&report, "tag_entities"), inserted(1));
    assert_eq!(counts(&report, "entity_fields"), inserted(1));
    assert_eq!(counts(&report, "entity_templates"), inserted(1));
    assert_eq!(counts(&report, "template_fields"), inserted(1));
    assert_eq!(counts(&report, "attachments"), inserted(2));
    assert_eq!(counts(&report, "thumbnails"), inserted(1));
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("maintenance"));
    assert!(report.warnings[0].contains('1'));
    let printed = report.to_string();
    assert!(printed.contains("entities"), "{printed}");
    assert!(printed.contains("maintenance"), "{printed}");
}

#[test]
fn maps_built_in_types_and_keeps_other_ids() {
    let h = Harness::new();
    h.import().unwrap();
    assert_eq!(h.entity(&h.ids.garage).entity_type_id, LOCATION_TYPE_ID);
    assert_eq!(h.entity(&h.ids.router).entity_type_id, ITEM_TYPE_ID);
    assert_eq!(h.entity(&h.ids.tote1).entity_type_id, h.ids.tote_type);
}

#[test]
fn parent_links_use_entity_children() {
    let h = Harness::new();
    h.import().unwrap();
    assert_eq!(h.entity(&h.ids.tote1).parent_id, Some(h.ids.garage.clone()));
    assert_eq!(h.entity(&h.ids.cable).parent_id, Some(h.ids.tote1.clone()));
    assert_eq!(h.entity(&h.ids.garage).parent_id, None);
}

#[test]
fn money_dates_and_flags_convert() {
    let h = Harness::new();
    h.import().unwrap();
    let router = h.entity(&h.ids.router);
    assert_eq!(router.purchase_price_cents, Cents(109_999));
    assert_eq!(
        router.purchase_date,
        Some(NaiveDate::from_ymd_opt(2021, 10, 23).unwrap())
    );
    assert!(router.insured);
    assert_eq!(router.asset_id, AssetId(7));
    let cable = h.entity(&h.ids.cable);
    assert_eq!(cable.purchase_price_cents, Cents(450));
    assert_eq!(cable.quantity, 3.0);
}

#[test]
fn tags_and_custom_fields_link() {
    let h = Harness::new();
    h.import().unwrap();
    let mut conn = h.conn();
    let router_tags: Vec<String> = svc::tag::for_entity(&mut conn, &h.ids.router)
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(router_tags, ["IOT"]);
    let general = svc::tag::get(&mut conn, &h.ids.general).unwrap().unwrap();
    assert_eq!(general.parent_id, Some(h.ids.iot.clone()));
    assert_eq!(general.color.as_deref(), Some("#ff0000"));
    let fields = svc::entity_field::for_entity(&mut conn, &h.ids.router).unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].text_value.as_deref(), Some("AX1800"));
    assert_eq!(fields[0].kind, FieldKind::Text);
}

#[test]
fn templates_link_to_types_locations_and_fields() {
    let h = Harness::new();
    h.import().unwrap();
    let mut conn = h.conn();
    let tote_type: EntityType = entity_types::table
        .find(&h.ids.tote_type)
        .select(EntityType::as_select())
        .first(&mut conn)
        .unwrap();
    assert_eq!(tote_type.default_template_id, Some(h.ids.template.clone()));
    assert!(tote_type.is_location);
    assert_eq!(tote_type.icon.as_deref(), Some("mdi-package"));
    let template: EntityTemplate = entity_templates::table
        .find(&h.ids.template)
        .select(EntityTemplate::as_select())
        .first(&mut conn)
        .unwrap();
    assert_eq!(template.location_id, Some(h.ids.garage.clone()));
    assert_eq!(
        template.default_tag_ids,
        Some(format!("[\"{}\"]", h.ids.iot))
    );
    assert!(template.include_purchase_fields);
    let field: TemplateField = template_fields::table
        .find(&h.ids.template_field)
        .select(TemplateField::as_select())
        .first(&mut conn)
        .unwrap();
    assert_eq!(field.template_id, h.ids.template);
    assert_eq!(field.kind, FieldKind::Text);
}

#[test]
fn originals_are_content_addressed_and_thumbnails_stored_at_500() {
    let h = Harness::new();
    let report = h.import().unwrap();
    let jpeg = fs::read(h.backup.path().join("attachments").join(&h.ids.photo)).unwrap();
    let webp = fs::read(h.backup.path().join("attachments").join(&h.ids.thumb)).unwrap();
    let original = h.data.path().join("originals").join(&h.ids.jpeg_sha256);
    assert_eq!(fs::read(original).unwrap(), jpeg);

    let mut conn = h.conn();
    let photo = svc::attachment::get(&mut conn, &h.ids.photo)
        .unwrap()
        .unwrap();
    assert_eq!(photo.sha256, h.ids.jpeg_sha256);
    assert_eq!(photo.size_bytes, i64::try_from(jpeg.len()).unwrap());
    assert_eq!(photo.kind, AttachmentKind::Photo);
    assert!(photo.is_primary);
    assert_eq!(photo.mime_type, "image/jpeg");
    assert_eq!(photo.title, "image.jpg");
    let manual = svc::attachment::get(&mut conn, &h.ids.manual)
        .unwrap()
        .unwrap();
    assert_eq!(manual.kind, AttachmentKind::Manual);
    assert!(
        svc::attachment::get(&mut conn, &h.ids.thumb)
            .unwrap()
            .is_none()
    );

    let thumb = svc::attachment::thumbnail(&mut conn, &h.ids.photo, 500)
        .unwrap()
        .unwrap();
    assert_eq!((thumb.width, thumb.height), (4, 3));
    assert_eq!(thumb.mime_type, "image/webp");
    assert_eq!(thumb.data, webp);
    assert_eq!(report.originals_written, 2);
}

#[test]
fn rerunning_is_idempotent_and_never_deletes() {
    let h = Harness::new();
    h.import().unwrap();
    let before = row_counts(&mut h.conn());
    let second = h.import().unwrap();
    for (table, c) in &second.tables {
        assert_eq!(c.inserted, 0, "{table} inserted rows on the second run");
    }
    assert_eq!(counts(&second, "entities").updated, 4);
    assert_eq!(counts(&second, "tag_entities").skipped, 1);
    assert_eq!(row_counts(&mut h.conn()), before);
    assert_eq!(second.originals_written, 0);
    assert_eq!(h.entity(&h.ids.cable).parent_id, Some(h.ids.tote1.clone()));
}

#[test]
fn zip_and_directory_sources_agree() {
    let h = Harness::new();
    h.import().unwrap();

    let zip_dir = tempfile::tempdir().unwrap();
    let zip_path = zip_dir.path().join("backup.zip");
    MiniBackup::write_zip(&zip_path);
    let zip_db = TestDb::new();
    let mut zip_conn = zip_db.pool.get().unwrap();
    let mut zip_source = source::open(&zip_path).unwrap();
    let data = tempfile::tempdir().unwrap();
    import_backup(&mut zip_conn, zip_source.as_mut(), data.path()).unwrap();

    let mut dir_conn = h.conn();
    assert_eq!(all_entities(&mut zip_conn), all_entities(&mut dir_conn));
    assert_eq!(
        all_attachments(&mut zip_conn),
        all_attachments(&mut dir_conn)
    );
}

#[test]
fn refuses_other_schema_versions() {
    let h = Harness::new();
    h.edit_table("manifest", |m| m["schemaVersion"] = 2.into());
    let err = h.import().unwrap_err();
    assert!(format!("{err:#}").contains("schemaVersion 2"), "{err:#}");
    assert_eq!(all_entities(&mut h.conn()), []);
}

#[test]
fn missing_optional_tables_are_fine() {
    let h = Harness::new();
    fs::remove_file(h.backup.path().join("entity_fields.json")).unwrap();
    fs::remove_file(h.backup.path().join("entity_templates.json")).unwrap();
    let report = h.import().unwrap();
    assert_eq!(counts(&report, "entity_fields"), TableCounts::default());
    assert_eq!(counts(&report, "entity_templates"), TableCounts::default());
    assert_eq!(counts(&report, "entities"), inserted(4));
}

#[test]
fn an_unknown_parent_becomes_a_root_with_a_warning() {
    let h = Harness::new();
    let cable_id = h.ids.cable.clone();
    h.edit_table("entities", |rows| {
        Harness::row(rows, &cable_id)["entity_children"] = "missing-id".into();
    });
    let report = h.import().unwrap();
    assert_eq!(h.entity(&h.ids.cable).parent_id, None);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains(&h.ids.cable) && w.contains("missing-id")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn an_attachment_without_a_blob_is_skipped_with_a_warning() {
    let h = Harness::new();
    fs::remove_file(h.backup.path().join("attachments").join(&h.ids.manual)).unwrap();
    let report = h.import().unwrap();
    let mut conn = h.conn();
    assert!(
        svc::attachment::get(&mut conn, &h.ids.manual)
            .unwrap()
            .is_none()
    );
    assert_eq!(counts(&report, "attachments").skipped, 1);
    assert!(
        report.warnings.iter().any(|w| w.contains(&h.ids.manual)),
        "{:?}",
        report.warnings
    );
    assert!(
        svc::attachment::get(&mut conn, &h.ids.photo)
            .unwrap()
            .is_some()
    );
}

/// Not part of the suite: writes the mini backup to the shared cargo target dir
/// so it can be imported manually with `cargo run -- import <path>`. Run with
/// `cargo test --test import -- --ignored write_mini_backup_to_target_dir`.
#[test]
#[ignore]
fn write_mini_backup_to_target_dir() {
    let dir = Path::new("/home/.build/cargo-target/mini-backup");
    fs::create_dir_all(dir).unwrap();
    MiniBackup::write_dir(dir);
    println!("wrote mini backup to {}", dir.display());
}

#[test]
fn the_homebox_path_is_ignored() {
    let h = Harness::new();
    let photo_id = h.ids.photo.clone();
    h.edit_table("attachments", |rows| {
        Harness::row(rows, &photo_id)["path"] = "bogus/path".into();
    });
    let report = h.import().unwrap();
    let photo = svc::attachment::get(&mut h.conn(), &h.ids.photo)
        .unwrap()
        .unwrap();
    assert_eq!(photo.sha256, h.ids.jpeg_sha256);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("maintenance"));
}

#[test]
fn an_attachment_id_with_path_separators_is_skipped() {
    let h = Harness::new();
    let manual_id = h.ids.manual.clone();
    let evil = "../../etc/passwd";
    h.edit_table("attachments", |rows| {
        Harness::row(rows, &manual_id)["id"] = evil.into();
    });
    let report = h.import().unwrap();
    let mut conn = h.conn();
    assert!(svc::attachment::get(&mut conn, evil).unwrap().is_none());
    assert!(
        svc::attachment::get(&mut conn, &h.ids.photo)
            .unwrap()
            .is_some()
    );
    assert_eq!(counts(&report, "attachments").skipped, 1);
    // Rejected by name, not merely "no blob": the traversal target is never read.
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains(evil) && w.contains("not a plain file name")),
        "{:?}",
        report.warnings
    );
    // Only the photo's original was written, and nothing else under the data dir.
    assert_eq!(dir_names(h.data.path()), ["originals"]);
    assert_eq!(
        dir_names(&h.data.path().join("originals")),
        [h.ids.jpeg_sha256.as_str()]
    );
}

#[test]
fn sources_refuse_attachment_ids_that_are_paths() {
    let h = Harness::new();
    let mut dir = DirSource::new(h.backup.path());
    let zip_dir = tempfile::tempdir().unwrap();
    let zip_path = zip_dir.path().join("backup.zip");
    MiniBackup::write_zip(&zip_path);
    let mut zip = source::open(&zip_path).unwrap();
    for id in ["", "..", "../manifest.json", "a/b", "a\\b"] {
        assert!(dir.read_attachment(id).is_err(), "dir accepted {id:?}");
        assert!(zip.read_attachment(id).is_err(), "zip accepted {id:?}");
    }
}

#[test]
fn a_photo_whose_thumbnail_blob_is_missing_still_imports() {
    let h = Harness::new();
    fs::remove_file(h.backup.path().join("attachments").join(&h.ids.thumb)).unwrap();
    let report = h.import().unwrap();
    let mut conn = h.conn();
    let photo = svc::attachment::get(&mut conn, &h.ids.photo)
        .unwrap()
        .unwrap();
    assert_eq!(photo.sha256, h.ids.jpeg_sha256);
    assert!(
        svc::attachment::thumbnail(&mut conn, &h.ids.photo, 500)
            .unwrap()
            .is_none()
    );
    let naming_thumb = report
        .warnings
        .iter()
        .filter(|w| w.contains(&h.ids.thumb))
        .count();
    assert_eq!(naming_thumb, 1, "{:?}", report.warnings);
}

#[test]
fn zip_with_a_single_top_level_folder_is_accepted() {
    let zips = tempfile::tempdir().unwrap();
    let import_zip = |path: &Path| {
        let db = TestDb::new();
        let data = tempfile::tempdir().unwrap();
        let mut conn = db.pool.get().unwrap();
        let mut source = source::open(path).unwrap();
        import_backup(&mut conn, source.as_mut(), data.path()).unwrap();
        all_entities(&mut conn)
            .into_iter()
            .map(|e| e.id)
            .collect::<Vec<_>>()
    };
    let plain = zips.path().join("plain.zip");
    MiniBackup::write_zip(&plain);
    let nested = zips.path().join("nested.zip");
    MiniBackup::write_zip_with_prefix(&nested, "homebox-backup/");
    let plain_ids = import_zip(&plain);
    assert_eq!(plain_ids.len(), 4);
    assert_eq!(import_zip(&nested), plain_ids);
}
