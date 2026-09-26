// Each test crate uses a different subset of the shared fixture builder.
#[allow(dead_code)]
mod support;

use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use axum_test::{TestResponse, TestServer};
use diesel::prelude::*;
use diesel_migrations::MigrationHarness;
use home_tracker::db::{self, ITEM_TYPE_ID, MIGRATIONS, TestDb};
use home_tracker::kinds::AttachmentKind;
use home_tracker::models::Thumbnail;
use home_tracker::routes::app_with_thumbnails;
use home_tracker::schema::{attachments, thumbnails};
use home_tracker::svc::blob::Blob;
use home_tracker::svc::fixtures::{self, SampleIds, jpeg, png, seed_sample};
use home_tracker::svc::thumbnail::{ThumbSize, allowed_size};
use home_tracker::svc::thumbnail_service::ThumbnailService;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::logs::Logs;
use tempfile::TempDir;
use tokio::task::JoinSet;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// Attachments are user-supplied bytes served from the app origin: the browser
/// must not sniff them into something executable, and an HTML or SVG original
/// opened directly must not run script with the app's origin.
fn assert_hardened(response: &TestResponse) {
    assert_eq!(header(response, "x-content-type-options"), "nosniff");
    assert_eq!(header(response, "content-security-policy"), "sandbox");
}

/// A seeded database, a data dir, and a server whose thumbnail service the
/// test can inspect. Keeps the temp dirs alive for the test's duration.
struct Fixture {
    server: TestServer,
    thumbs: Arc<ThumbnailService>,
    db: TestDb,
    data: TempDir,
    ids: SampleIds,
}

impl Fixture {
    fn new() -> Self {
        let db = TestDb::new();
        let ids = seed_sample(&mut db.pool.get().unwrap());
        let data = tempfile::tempdir().unwrap();
        fs::create_dir_all(data.path().join("originals")).unwrap();
        let thumbs = ThumbnailService::new(db.pool.clone(), data.path().to_path_buf());
        let server = TestServer::new(app_with_thumbnails(db.pool.clone(), thumbs.clone()));
        Self {
            server,
            thumbs,
            db,
            data,
            ids,
        }
    }

    fn original_path(&self, sha256: &str) -> PathBuf {
        self.data.path().join("originals").join(sha256)
    }

    /// Stores `bytes` as an original and inserts a photo attachment `id`
    /// pointing at it. Returns the sha256.
    fn add_file(&self, id: &str, title: &str, mime: &str, bytes: &[u8]) -> String {
        let sha256 = hex::encode(Sha256::digest(bytes));
        fs::write(self.original_path(&sha256), bytes).unwrap();
        let mut row = fixtures::attachment(
            id,
            &self.ids.drill,
            AttachmentKind::Photo,
            false,
            &sha256,
            bytes.len() as i64,
            100,
        );
        row.title = title.to_owned();
        row.mime_type = mime.to_owned();
        diesel::insert_into(attachments::table)
            .values(row)
            .execute(&mut self.db.pool.get().unwrap())
            .unwrap();
        sha256
    }

    /// A 1600×1200 JPEG photo titled `photo.jpg`.
    fn add_photo(&self, id: &str) -> String {
        self.add_file(id, "photo.jpg", "image/jpeg", &jpeg(1600, 1200))
    }

    /// The stored thumbnail sizes of attachment `id`'s original.
    fn thumb_sizes(&self, id: &str) -> Vec<i32> {
        thumbnails::table
            .filter(
                thumbnails::sha256.eq_any(
                    attachments::table
                        .find(id)
                        .select(attachments::sha256)
                        .into_boxed(),
                ),
            )
            .select(thumbnails::size)
            .order(thumbnails::size.asc())
            .load(&mut self.db.pool.get().unwrap())
            .unwrap()
    }
}

fn header(response: &TestResponse, name: &str) -> String {
    response.header(name).to_str().unwrap().to_owned()
}

fn dimensions(response: &TestResponse) -> (u32, u32) {
    let img = image::load_from_memory(response.as_bytes()).unwrap();
    (img.width(), img.height())
}

#[tokio::test]
async fn serves_the_original_with_headers() {
    let f = Fixture::new();
    let bytes = jpeg(64, 48);
    let sha = f.add_file("att-1", "photo.jpg", "image/jpeg", &bytes);

    let response = f.server.get("/attachments/att-1?v=abc").await;

    response.assert_status_ok();
    assert_eq!(response.as_bytes().as_ref(), bytes.as_slice());
    assert_eq!(header(&response, "content-type"), "image/jpeg");
    assert_eq!(
        header(&response, "content-disposition"),
        r#"inline; filename="photo.jpg""#
    );
    assert_eq!(header(&response, "cache-control"), IMMUTABLE);
    assert_eq!(header(&response, "etag"), format!("\"{sha}\""));
    assert_hardened(&response);
}

#[tokio::test]
async fn strips_quotes_and_control_characters_from_the_filename() {
    // Review focus 4: a hostile title must not inject headers or break quoting.
    let f = Fixture::new();
    f.add_file(
        "att-evil",
        "evil\".jpg\r\nX-Injected: 1",
        "image/jpeg",
        &jpeg(8, 8),
    );

    let response = f.server.get("/attachments/att-evil").await;

    response.assert_status_ok();
    assert_eq!(
        header(&response, "content-disposition"),
        r#"inline; filename="evil.jpg X-Injected: 1""#
    );
    assert!(response.maybe_header("x-injected").is_none());
}

#[tokio::test]
async fn non_ascii_filenames_get_an_rfc_6266_extended_parameter() {
    let f = Fixture::new();
    f.add_file("att-utf8", "Bohrmaschine ü.jpg", "image/jpeg", &jpeg(8, 8));

    let response = f.server.get("/attachments/att-utf8").await;

    response.assert_status_ok();
    assert_eq!(
        header(&response, "content-disposition"),
        "inline; filename=\"Bohrmaschine _.jpg\"; filename*=UTF-8''Bohrmaschine%20%C3%BC.jpg"
    );
}

#[tokio::test]
async fn unknown_attachment_is_404() {
    let f = Fixture::new();
    f.server
        .get("/attachments/nope")
        .expect_failure()
        .await
        .assert_status_not_found();
}

#[tokio::test]
async fn missing_original_file_is_404_not_500() {
    let f = Fixture::new();
    let sha = f.add_photo("att-gone");
    fs::remove_file(f.original_path(&sha)).unwrap();

    let response = f.server.get("/attachments/att-gone").expect_failure().await;

    response.assert_status_not_found();
    assert!(response.maybe_header("cache-control").is_none());
}

#[tokio::test]
async fn thumb_rejects_bad_sizes() {
    let f = Fixture::new();
    f.add_photo("att-1");
    for size in ["abc", "0", "-3", "1.5"] {
        f.server
            .get(&format!("/attachments/att-1/thumb/{size}"))
            .expect_failure()
            .await
            .assert_status_bad_request();
    }
    assert_eq!(f.thumbs.generations(), 0);
}

#[tokio::test]
async fn thumb_rounds_up_and_clamps() {
    let f = Fixture::new();
    let sha = f.add_photo("att-1");

    for (requested, served, dims) in [
        ("1", 300, (300, 225)),
        ("300", 300, (300, 225)),
        ("301", 500, (500, 375)),
        ("9999", 1200, (1200, 900)),
    ] {
        let response = f
            .server
            .get(&format!("/attachments/att-1/thumb/{requested}"))
            .await;
        response.assert_status_ok();
        assert_eq!(header(&response, "content-type"), "image/webp");
        assert_eq!(header(&response, "cache-control"), IMMUTABLE);
        assert_eq!(header(&response, "etag"), format!("\"{sha}-{served}\""));
        assert_eq!(dimensions(&response), dims, "requested {requested}");
        assert_hardened(&response);
    }
    assert_eq!(f.thumb_sizes("att-1"), [300, 500, 1200]);
    assert_eq!(f.thumbs.generations(), 3);
}

#[tokio::test]
async fn thumb_is_404_for_non_images() {
    let f = Fixture::new();
    fs::write(f.original_path(&"bb".repeat(32)), b"%PDF-1.4 mini").unwrap();

    let url = format!("/attachments/{}/thumb/300", f.ids.manual);
    f.server
        .get(&url)
        .expect_failure()
        .await
        .assert_status_not_found();
    assert_eq!(f.thumbs.generations(), 0);
}

#[tokio::test]
async fn thumb_is_case_insensitive_on_mime() {
    // Review focus 3: Homebox stores whatever MIME the uploader sent.
    let f = Fixture::new();
    f.add_file("att-upper", "a.jpg", "image/JPEG", &jpeg(640, 480));
    f.add_file("att-mixed", "b.png", " Image/Png ", &png(640, 480));

    for id in ["att-upper", "att-mixed"] {
        let response = f.server.get(&format!("/attachments/{id}/thumb/300")).await;
        response.assert_status_ok();
        assert_eq!(dimensions(&response), (300, 225), "{id}");
    }
}

#[tokio::test]
async fn thumb_is_generated_once_then_cached() {
    let f = Fixture::new();
    f.add_photo("att-1");

    let first = f.server.get("/attachments/att-1/thumb/500").await;
    let second = f.server.get("/attachments/att-1/thumb/500").await;

    first.assert_status_ok();
    second.assert_status_ok();
    assert_eq!(first.as_bytes(), second.as_bytes());
    assert_eq!(f.thumbs.generations(), 1);
    assert_eq!(f.thumb_sizes("att-1"), [500]);
}

/// Fires every `(url, copies)` request at once on the multi-threaded runtime
/// and returns the status codes.
async fn concurrently(server: &TestServer, requests: &[(&str, usize)]) -> Vec<u16> {
    let mut set = JoinSet::new();
    for (url, copies) in requests {
        for _ in 0..*copies {
            set.spawn(server.get(url).into_future());
        }
    }
    set.join_all()
        .await
        .iter()
        .map(|r| r.status_code().as_u16())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_misses_generate_once() {
    // Review focus 1.
    let f = Fixture::new();
    f.add_photo("att-1");

    let statuses = concurrently(&f.server, &[("/attachments/att-1/thumb/300", 8)]).await;
    assert_eq!(statuses, [200; 8]);
    assert_eq!(f.thumbs.generations(), 1);

    let statuses = concurrently(
        &f.server,
        &[
            ("/attachments/att-1/thumb/500", 4),
            ("/attachments/att-1/thumb/1200", 4),
        ],
    )
    .await;
    assert_eq!(statuses, [200; 8]);
    assert_eq!(f.thumbs.generations(), 3);
    assert_eq!(f.thumb_sizes("att-1"), [300, 500, 1200]);
}

#[tokio::test]
async fn thumb_for_a_missing_original_is_404() {
    // Review focus 2: the seeded photo's `aa…` original was never written.
    let f = Fixture::new();
    let url = format!("/attachments/{}/thumb/300", f.ids.photo);
    f.server
        .get(&url)
        .expect_failure()
        .await
        .assert_status_not_found();
    assert_eq!(f.thumbs.generations(), 0);
}

#[tokio::test]
async fn imported_homebox_thumbnail_is_served_without_generation() {
    let f = Fixture::new();
    let sha = f.add_photo("att-1");
    let stored = b"RIFF\x0c\0\0\0WEBPVP8 fake".to_vec();
    diesel::insert_into(thumbnails::table)
        .values(Thumbnail {
            sha256: sha.clone(),
            size: 500,
            mime_type: "image/webp".to_owned(),
            width: 500,
            height: 375,
            data: stored.clone(),
            created_at: fixtures::at(200),
        })
        .execute(&mut f.db.pool.get().unwrap())
        .unwrap();

    let response = f.server.get("/attachments/att-1/thumb/500").await;

    response.assert_status_ok();
    assert_eq!(response.as_bytes().as_ref(), stored.as_slice());
    assert_eq!(header(&response, "content-type"), "image/webp");
    assert_eq!(header(&response, "etag"), format!("\"{sha}-500\""));
    assert_hardened(&response);
    assert_eq!(f.thumbs.generations(), 0);
}

#[tokio::test]
async fn graphql_urls_resolve_on_the_http_routes() {
    // The seam between the GraphQL URL formatting and the HTTP routes: the
    // strings the SPA is handed must be fetchable verbatim, `?v=` and all.
    let f = Fixture::new();
    let bytes = jpeg(1600, 1200);
    f.add_file("att-real", "photo.jpg", "image/jpeg", &bytes);
    diesel::update(attachments::table.filter(attachments::entity_id.eq(&f.ids.drill)))
        .set(attachments::is_primary.eq(attachments::id.eq("att-real")))
        .execute(&mut f.db.pool.get().unwrap())
        .unwrap();

    let response = f
        .server
        .post("/graphql")
        .json(&json!({
            "query": "query($id: ID!) { entity(id: $id) { primaryPhoto { url thumbnailUrl(size: 300) } } }",
            "variables": { "id": f.ids.drill },
        }))
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    let photo = &body["data"]["entity"]["primaryPhoto"];
    let url = photo["url"].as_str().unwrap();
    let thumbnail_url = photo["thumbnailUrl"].as_str().unwrap();
    assert!(url.starts_with("/attachments/att-real?v="), "{url}");
    assert!(
        thumbnail_url.starts_with("/attachments/att-real/thumb/300?v="),
        "{thumbnail_url}"
    );

    let original = f.server.get(url).await;
    original.assert_status_ok();
    assert_eq!(original.as_bytes().as_ref(), bytes.as_slice());

    let thumb = f.server.get(thumbnail_url).await;
    thumb.assert_status_ok();
    assert_eq!(header(&thumb, "content-type"), "image/webp");
    assert_eq!(dimensions(&thumb), (300, 225));
}

#[tokio::test]
async fn originals_are_not_compressed() {
    // Originals are served byte-for-byte under their sha256 ETag; re-encoding
    // them would only waste CPU on already-compressed photos.
    let f = Fixture::new();
    let bytes = "plain text that is long enough to be worth compressing. ".repeat(20);
    f.add_file("att-text", "notes.txt", "text/plain", bytes.as_bytes());

    let response = f
        .server
        .get("/attachments/att-text")
        .add_header("accept-encoding", "gzip")
        .await;

    response.assert_status_ok();
    assert!(
        response.maybe_header("content-encoding").is_none(),
        "originals must not be compressed"
    );
    assert_eq!(response.as_bytes().as_ref(), bytes.as_bytes());

    // The GraphQL endpoint is still compressed.
    let graphql = f
        .server
        .post("/graphql")
        .add_header("accept-encoding", "gzip")
        .json(&json!({ "query": "{ entityTypes { id name description icon isLocation } }" }))
        .await;
    graphql.assert_status_ok();
    assert_eq!(header(&graphql, "content-encoding"), "gzip");
}

#[tokio::test]
async fn generation_is_bounded_by_the_semaphore() {
    let f = Fixture::new();
    let cores = thread::available_parallelism().map_or(2, |n| n.get());
    assert_eq!(f.thumbs.permits(), cores);

    f.add_photo("att-1");
    let response = f.server.get("/attachments/att-1/thumb/300").await;
    response.assert_status_ok();
    assert_eq!(f.thumbs.generations(), 1);
}

#[tokio::test]
async fn undecodable_original_is_404_once_then_cached() {
    let f = Fixture::new();
    let (logs, _guard) = Logs::capture();
    // JPEG magic, so it is thumbnailable by type, but no decodable image.
    f.add_file(
        "att-bad",
        "photo.jpg",
        "image/jpeg",
        b"\xFF\xD8\xFF\xE0 not really a jpeg",
    );

    for size in [300, 300, 500] {
        f.server
            .get(&format!("/attachments/att-bad/thumb/{size}"))
            .expect_failure()
            .await
            .assert_status_not_found();
    }

    assert_eq!(f.thumbs.generations(), 0);
    assert!(f.thumb_sizes("att-bad").is_empty());
    let text = logs.text();
    assert_eq!(
        text.matches("could not be decoded").count(),
        1,
        "decoded more than once: {text}"
    );
    assert!(text.contains("att-bad"), "{text}");
}

#[tokio::test]
async fn missing_original_logs_a_warning() {
    let f = Fixture::new();
    let sha = f.add_photo("att-gone");
    fs::remove_file(f.original_path(&sha)).unwrap();
    let (logs, _guard) = Logs::capture();

    f.server
        .get("/attachments/att-gone/thumb/300")
        .expect_failure()
        .await
        .assert_status_not_found();

    let text = logs.text();
    let line = text
        .lines()
        .find(|l| l.contains("original file missing"))
        .unwrap_or_else(|| panic!("no warning in {text:?}"));
    assert!(line.contains("WARN"), "{line}");
    assert!(line.contains("att-gone"), "{line}");
}

#[tokio::test]
async fn thumb_size_newtype_is_used_end_to_end() {
    let size: ThumbSize = allowed_size(301).unwrap();
    assert_eq!(size.get(), 500);
    let f = Fixture::new();
    let blob = Blob {
        sha256: f.add_photo("att-1"),
        mime_type: "image/jpeg".to_owned(),
    };

    let thumb = f
        .thumbs
        .get_or_generate(&blob, size)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(thumb.size, 500);
    assert_eq!((thumb.width, thumb.height), (500, 375));
}

#[tokio::test]
async fn read_failures_are_500_without_filesystem_paths() {
    // An original that exists but cannot be read: a symlink to itself.
    let f = Fixture::new();
    let sha = f.add_photo("att-loop");
    let path = f.original_path(&sha);
    fs::remove_file(&path).unwrap();
    unix_fs::symlink(&path, &path).unwrap();
    let data_dir = f.data.path().to_str().unwrap().to_owned();

    for url in ["/attachments/att-loop", "/attachments/att-loop/thumb/300"] {
        let response = f.server.get(url).expect_failure().await;
        response.assert_status_internal_server_error();
        let body = response.text();
        assert!(body.contains("att-loop"), "{url}: {body}");
        assert!(!body.contains(&data_dir), "{url} leaks a path: {body}");
        assert!(!body.contains(&sha), "{url} leaks the file name: {body}");
    }
}

#[tokio::test]
async fn two_attachments_sharing_bytes_share_one_thumbnail() {
    let f = Fixture::new();
    let bytes = jpeg(640, 480);
    let sha = f.add_file("att-1", "a.jpg", "image/jpeg", &bytes);
    f.add_file("att-2", "b.jpg", "image/jpeg", &bytes);

    for id in ["att-1", "att-2"] {
        let response = f.server.get(&format!("/attachments/{id}/thumb/300")).await;
        response.assert_status_ok();
        assert_eq!(header(&response, "etag"), format!("\"{sha}-300\""));
    }

    assert_eq!(f.thumbs.generations(), 1);
    let rows: i64 = thumbnails::table
        .count()
        .get_result(&mut f.db.pool.get().unwrap())
        .unwrap();
    // The sample photo's imported 500px thumbnail plus the one shared 300px.
    assert_eq!(rows, 2);
    assert_eq!(f.thumb_sizes("att-2"), [300]);
}

/// The migration that re-keys thumbnails by content hash.
const THUMBNAILS_BY_SHA: &str = "2026-09-26-100000_thumbnails_by_sha";

#[tokio::test]
async fn existing_thumbnails_survive_the_migration() {
    // Review focus 1: a database written before the re-keying keeps serving
    // its stored thumbnails at the same URL with the same bytes.
    let dir = tempfile::tempdir().unwrap();
    let pool = db::build_pool(dir.path().join("old.sqlite").to_str().unwrap()).unwrap();
    let mut conn = pool.get().unwrap();
    // In order: everything before the re-keying, then (after seeding the old
    // shape) the re-keying itself, then everything after it.
    let pending = conn.pending_migrations(MIGRATIONS).unwrap();
    let at = pending
        .iter()
        .position(|m| m.name().to_string() == THUMBNAILS_BY_SHA)
        .expect("the re-keying migration is embedded");
    let (earlier, rest) = pending.split_at(at);
    for migration in earlier {
        conn.run_migration(migration.as_ref()).unwrap();
    }

    let sha = "5e".repeat(32);
    diesel::insert_into(home_tracker::schema::entities::table)
        .values(fixtures::entity("e-1", "Drill", ITEM_TYPE_ID, None, 1))
        .execute(&mut conn)
        .unwrap();
    for id in ["att-1", "att-2"] {
        diesel::insert_into(attachments::table)
            .values(fixtures::attachment(
                id,
                "e-1",
                AttachmentKind::Photo,
                id == "att-1",
                &sha,
                10,
                2,
            ))
            .execute(&mut conn)
            .unwrap();
        // Old shape: one row per attachment, even for identical bytes.
        diesel::sql_query(
            "INSERT INTO thumbnails (attachment_id, size, mime_type, width, height, data) \
             VALUES (?, 500, 'image/webp', 10, 8, CAST('webp-bytes' AS BLOB))",
        )
        .bind::<diesel::sql_types::Text, _>(id)
        .execute(&mut conn)
        .unwrap();
    }

    conn.run_migration(rest[0].as_ref()).unwrap();
    db::run_migrations(&mut conn).unwrap();
    assert!(conn.pending_migrations(MIGRATIONS).unwrap().is_empty());

    let rows: Vec<(String, i32, i32, i32, Vec<u8>)> = thumbnails::table
        .select((
            thumbnails::sha256,
            thumbnails::size,
            thumbnails::width,
            thumbnails::height,
            thumbnails::data,
        ))
        .load(&mut conn)
        .unwrap();
    assert_eq!(rows, [(sha.clone(), 500, 10, 8, b"webp-bytes".to_vec())]);

    // No original is on disk: only the migrated row can answer.
    let data = tempfile::tempdir().unwrap();
    let thumbs = ThumbnailService::new(pool.clone(), data.path().to_path_buf());
    let server = TestServer::new(app_with_thumbnails(pool.clone(), thumbs.clone()));
    for id in ["att-1", "att-2"] {
        let response = server.get(&format!("/attachments/{id}/thumb/500")).await;
        response.assert_status_ok();
        assert_eq!(response.as_bytes().as_ref(), b"webp-bytes");
        assert_eq!(header(&response, "etag"), format!("\"{sha}-500\""));
    }
    assert_eq!(thumbs.generations(), 0);
}
