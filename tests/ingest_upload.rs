//! `POST /api/ingest/items/{item_id}/photos` (staging a photo for AI ingest)
//! and the `/ingest/photos/{id}` routes that serve it, which share their
//! header-emitting code with the attachment routes.

// Each test crate uses a different subset of the shared support.
#[allow(dead_code)]
mod support;

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum_test::{TestResponse, TestServer};
use diesel::prelude::*;
use home_tracker::api::upload::{MAX_UPLOAD_BYTES, MULTIPART_OVERHEAD_BYTES};
use home_tracker::db::TestDb;
use home_tracker::models::IngestItem;
use home_tracker::routes::{app_with_actor, app_with_thumbnails};
use home_tracker::schema::{ingest_photos, thumbnails};
use home_tracker::svc::fixtures::{self, SampleIds, jpeg, seed_sample};
use home_tracker::svc::ingest;
use home_tracker::svc::thumbnail_service::ThumbnailService;
use serde_json::Value;
use sha2::{Digest, Sha256};
use support::upload::{
    Untouchable, error_message, json_body, multipart_type, photo_form, read_only, streamed_request,
};
use tempfile::TempDir;
use tokio::time;
use tower::ServiceExt;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// A seeded database, a data dir, a collecting batch with one item, and a
/// server whose thumbnail service the test can inspect.
struct Fixture {
    server: TestServer,
    thumbs: Arc<ThumbnailService>,
    db: TestDb,
    data: TempDir,
    ids: SampleIds,
    item: IngestItem,
}

impl Fixture {
    fn new() -> Self {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let (_, item) = fixtures::ingest_batch(&mut conn, Some(&ids.garage));
        drop(conn);
        let data = tempfile::tempdir().unwrap();
        let thumbs = ThumbnailService::new(db.pool.clone(), data.path().to_path_buf());
        let server = TestServer::new(app_with_thumbnails(db.pool.clone(), thumbs.clone()));
        Self {
            server,
            thumbs,
            db,
            data,
            ids,
            item,
        }
    }

    /// A second router over the same state, for requests axum-test cannot
    /// shape (a lying `Content-Length`).
    fn router(&self) -> Router {
        app_with_thumbnails(self.db.pool.clone(), self.thumbs.clone())
    }

    /// Every file name under `originals/`, sorted; empty when it does not exist.
    fn originals(&self) -> Vec<String> {
        let Ok(dir) = fs::read_dir(self.data.path().join("originals")) else {
            return Vec::new();
        };
        let mut names: Vec<String> = dir
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    async fn stage(&self, item_id: &str, bytes: Vec<u8>) -> TestResponse {
        self.server
            .post(&stage_uri(item_id))
            .multipart(photo_form(bytes))
            .await
    }

    async fn upload_attachment(&self, entity_id: &str, bytes: Vec<u8>) -> TestResponse {
        self.server
            .post(&format!("/api/upload/{entity_id}"))
            .multipart(photo_form(bytes))
            .await
    }

    /// How many staged photo rows exist.
    fn staged_rows(&self) -> i64 {
        ingest_photos::table
            .count()
            .get_result(&mut self.db.pool.get().unwrap())
            .unwrap()
    }

    /// The stored thumbnail sizes of the original with `sha256`.
    fn thumb_sizes(&self, sha256: &str) -> Vec<i32> {
        thumbnails::table
            .filter(thumbnails::sha256.eq(sha256))
            .select(thumbnails::size)
            .order(thumbnails::size.asc())
            .load(&mut self.db.pool.get().unwrap())
            .unwrap()
    }
}

/// The staging route of item `item_id`.
fn stage_uri(item_id: &str) -> String {
    format!("/api/ingest/items/{item_id}/photos")
}

fn header_text(response: &TestResponse, name: &str) -> String {
    response.header(name).to_str().unwrap().to_owned()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// `url` without its query string.
fn path_of(url: &Value) -> &str {
    let url = url.as_str().unwrap();
    url.split_once('?').map_or(url, |(path, _)| path)
}

#[tokio::test]
async fn stages_a_jpeg_and_serves_it_at_all_sizes() {
    let f = Fixture::new();
    let bytes = jpeg(1600, 1200);
    let sha = sha256_hex(&bytes);

    let response = f.stage(&f.item.id, bytes.clone()).await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let id = body["id"].as_str().unwrap();
    assert_eq!(body["position"], 0);
    assert_eq!(body["status"], "PENDING");
    assert_eq!(body["title"], "photo.jpg");
    assert_eq!(body["mimeType"], "image/jpeg");
    assert_eq!(body["sizeBytes"], bytes.len());
    assert_eq!(
        body["url"],
        format!("/ingest/photos/{id}?v={}", &sha[..12]),
        "{body}"
    );
    assert_eq!(
        body["thumbnailUrl"],
        format!("/ingest/photos/{id}/thumb/500?v={}", &sha[..12]),
        "{body}"
    );
    assert_eq!(f.originals(), [sha.as_str()]);
    assert_eq!(f.thumb_sizes(&sha), [300], "the 300px thumbnail is made");

    let second: Value = f.stage(&f.item.id, jpeg(8, 6)).await.json();
    assert_eq!(second["position"], 1);

    let original = f.server.get(path_of(&body["url"])).await;
    original.assert_status_ok();
    assert_eq!(original.as_bytes().as_ref(), bytes.as_slice());
    assert_eq!(header_text(&original, "content-type"), "image/jpeg");
    assert_eq!(header_text(&original, "etag"), format!("\"{sha}\""));
    assert_eq!(header_text(&original, "cache-control"), IMMUTABLE);
    assert_eq!(header_text(&original, "x-content-type-options"), "nosniff");
    assert_eq!(header_text(&original, "content-security-policy"), "sandbox");
    assert_eq!(
        header_text(&original, "content-disposition"),
        r#"inline; filename="photo.jpg""#
    );

    for size in [300, 500, 1200] {
        let thumb = f
            .server
            .get(&format!("/ingest/photos/{id}/thumb/{size}"))
            .await;
        thumb.assert_status_ok();
        assert_eq!(header_text(&thumb, "content-type"), "image/webp");
        assert_eq!(header_text(&thumb, "etag"), format!("\"{sha}-{size}\""));
        assert_eq!(header_text(&thumb, "x-content-type-options"), "nosniff");
    }
    f.server
        .get(&format!("/ingest/photos/{id}/thumb/0"))
        .expect_failure()
        .await
        .assert_status_bad_request();
    for missing in ["/ingest/photos/nope", "/ingest/photos/nope/thumb/300"] {
        f.server
            .get(missing)
            .expect_failure()
            .await
            .assert_status_not_found();
    }
}

#[tokio::test]
async fn identical_bytes_shared_with_an_attachment_reuse_the_file() {
    // Review Focus 1: staging bytes an attachment already has adds a row,
    // not a file, and shares the thumbnails; removing the staged photo
    // leaves the attachment's file in place.
    let f = Fixture::new();
    let bytes = jpeg(640, 480);
    let sha = sha256_hex(&bytes);
    let attachment: Value = f
        .upload_attachment(&f.ids.screws, bytes.clone())
        .await
        .json();
    assert_eq!(f.originals(), [sha.as_str()]);

    let staged = f.stage(&f.item.id, bytes).await;

    staged.assert_status(StatusCode::CREATED);
    let staged: Value = staged.json();
    assert_eq!(f.originals(), [sha.as_str()], "no second copy");
    assert_eq!(f.thumb_sizes(&sha), [300], "one shared 300px thumbnail");
    let attachment_thumb = format!("{}/thumb/1200", path_of(&attachment["url"]));
    let staged_thumb = format!("{}/thumb/1200", path_of(&staged["url"]));
    f.server.get(&attachment_thumb).await.assert_status_ok();
    f.server.get(&staged_thumb).await.assert_status_ok();
    assert_eq!(f.thumbs.generations(), 1, "the 1200px thumbnail is shared");
    assert_eq!(f.thumb_sizes(&sha), [300, 1200]);

    let mut conn = f.db.pool.get().unwrap();
    ingest::remove_photo(&mut conn, f.data.path(), staged["id"].as_str().unwrap()).unwrap();
    drop(conn);

    assert_eq!(f.originals(), [sha.as_str()], "the attachment keeps it");
    f.server
        .get(path_of(&attachment["url"]))
        .await
        .assert_status_ok();
    f.server
        .get(path_of(&staged["url"]))
        .expect_failure()
        .await
        .assert_status_not_found();
}

#[tokio::test]
async fn rejects_html_as_415_and_leaves_nothing() {
    let f = Fixture::new();
    let html = b"<!doctype html><html><script>alert(1)</script></html>".to_vec();

    let response = f.stage(&f.item.id, html).await;

    response.assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(!error_message(&response).is_empty());
    assert_eq!(f.originals(), Vec::<String>::new());
    assert_eq!(f.staged_rows(), 0);
}

#[tokio::test]
async fn unknown_item_is_404() {
    let f = Fixture::new();

    let response = f.stage("no-such-item", jpeg(8, 6)).await;

    response.assert_status_not_found();
    assert_eq!(error_message(&response), "ingest item not found");
    assert_eq!(f.originals(), Vec::<String>::new());
    assert_eq!(f.staged_rows(), 0);
}

#[tokio::test]
async fn a_submitted_batch_refuses_more_photos_with_409() {
    let f = Fixture::new();
    let first = jpeg(8, 6);
    f.stage(&f.item.id, first.clone())
        .await
        .assert_status(StatusCode::CREATED);
    ingest::submit(&mut f.db.pool.get().unwrap(), &f.item.batch_id).unwrap();

    let response = f.stage(&f.item.id, jpeg(9, 6)).await;

    response.assert_status(StatusCode::CONFLICT);
    assert_eq!(
        error_message(&response),
        "This batch is no longer collecting photos"
    );
    assert_eq!(f.originals(), [sha256_hex(&first)]);
    assert_eq!(f.staged_rows(), 1);
}

#[tokio::test]
async fn read_only_actor_is_403() {
    // Refused before the body is read: a body that never yields would hang a
    // handler that reads it, and records that it was polled.
    let f = Fixture::new();
    let router = app_with_actor(f.db.pool.clone(), f.data.path().to_path_buf(), read_only());
    let polled = Arc::new(AtomicBool::new(false));
    let request = streamed_request(&stage_uri(&f.item.id), Untouchable(Arc::clone(&polled)));

    let response = time::timeout(Duration::from_secs(2), router.oneshot(request))
        .await
        .expect("the handler waited on the body")
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(json_body(response).await["error"].is_string());
    assert!(!polled.load(Ordering::SeqCst), "the body was polled");
    assert_eq!(f.originals(), Vec::<String>::new());
    assert_eq!(f.staged_rows(), 0);
}

#[tokio::test]
async fn staged_originals_are_not_compressed() {
    // Served byte-for-byte under their sha256 ETag, like attachments. Text
    // (which the compression layer would compress) proves the route sits
    // outside it; the row is inserted by hand since uploads take images only.
    let f = Fixture::new();
    let bytes = "plain text that is long enough to be worth compressing. ".repeat(20);
    let sha = sha256_hex(bytes.as_bytes());
    fs::create_dir_all(f.data.path().join("originals")).unwrap();
    fs::write(f.data.path().join("originals").join(&sha), &bytes).unwrap();
    let photo = fixtures::ingest_photo(
        &mut f.db.pool.get().unwrap(),
        &f.item.id,
        &sha,
        "text/plain",
    );

    let response = f
        .server
        .get(&format!("/ingest/photos/{}", photo.id))
        .add_header("accept-encoding", "gzip")
        .await;

    response.assert_status_ok();
    assert!(
        response.maybe_header("content-encoding").is_none(),
        "staged originals must not be compressed"
    );
    assert_eq!(
        header_text(&response, "content-length"),
        bytes.len().to_string()
    );
    assert_eq!(response.as_bytes().as_ref(), bytes.as_bytes());
}

#[tokio::test]
async fn over_limit_is_413_json() {
    let f = Fixture::new();
    // Refused on the declared length alone, before any handler runs.
    let request = Request::post(stage_uri(&f.item.id))
        .header(header::CONTENT_TYPE, multipart_type())
        .header(
            header::CONTENT_LENGTH,
            MAX_UPLOAD_BYTES + MULTIPART_OVERHEAD_BYTES + 1,
        )
        .body(Body::empty())
        .unwrap();

    let response = f.router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(json_body(response).await["error"].is_string());

    // Within the body limit, so the file's own size is what is refused, and
    // a file of exactly the limit fits: the 2 MiB default is not in force.
    let mut over = jpeg(8, 6);
    over.resize(MAX_UPLOAD_BYTES + 1, 0);
    let response = f.stage(&f.item.id, over).await;
    response.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!error_message(&response).is_empty());
    assert_eq!(f.originals(), Vec::<String>::new());
    let mut exact = jpeg(8, 6);
    exact.resize(MAX_UPLOAD_BYTES, 0);
    f.stage(&f.item.id, exact)
        .await
        .assert_status(StatusCode::CREATED);
}

/// `response`'s headers, which must be the same whichever route served the blob.
fn served_headers(response: &TestResponse) -> HeaderMap {
    response.assert_status_ok();
    response.headers().clone()
}

#[tokio::test]
async fn attachment_routes_still_behave_the_same() {
    // tests/attachments.rs pins the attachment routes; this pins that the
    // ingest photo routes answer the same blob with the same headers.
    let f = Fixture::new();
    let bytes = jpeg(640, 480);
    let attachment: Value = f
        .upload_attachment(&f.ids.screws, bytes.clone())
        .await
        .json();
    let staged: Value = f.stage(&f.item.id, bytes).await.json();
    let attachment_url = path_of(&attachment["url"]);
    let staged_url = path_of(&staged["url"]);

    let pairs = [
        (attachment_url.to_owned(), staged_url.to_owned()),
        (
            format!("{attachment_url}/thumb/500"),
            format!("{staged_url}/thumb/500"),
        ),
    ];
    for (attachment_url, staged_url) in pairs {
        let from_attachment = served_headers(&f.server.get(&attachment_url).await);
        let from_staged = served_headers(&f.server.get(&staged_url).await);
        assert_eq!(
            from_attachment, from_staged,
            "{attachment_url} vs {staged_url}"
        );
        assert!(
            from_attachment.contains_key(header::ETAG),
            "{from_attachment:?}"
        );
    }
}
