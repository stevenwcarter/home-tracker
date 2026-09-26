//! `POST /api/upload/{entity_id}`: the multipart photo upload, its body limit,
//! and the actor seam that refuses a read-only caller before the body is read.

use std::fs;
use std::io::{Cursor, ErrorKind};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::Router;
use axum::body::{self, Body};
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use axum_test::multipart::{MultipartForm, Part};
use axum_test::{TestResponse, TestServer};
use home_tracker::db::TestDb;
use home_tracker::graphql::context::{Actor, Role};
use home_tracker::routes::{app, app_with_actor};
use home_tracker::svc::fixtures::{SampleIds, jpeg, seed_sample};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{self, AsyncRead, AsyncReadExt, ReadBuf};
use tokio::time;
use tokio_util::io::ReaderStream;
use tower::ServiceExt;

const MIB: usize = 1024 * 1024;
const BOUNDARY: &str = "home-tracker-test-boundary";

/// A seeded database, an empty data dir, and a server over both.
struct Fixture {
    server: TestServer,
    db: TestDb,
    data: TempDir,
    ids: SampleIds,
}

impl Fixture {
    fn new() -> Self {
        Self::build(|db, data| app(db.pool.clone(), data))
    }

    fn with_actor(actor: Actor) -> Self {
        Self::build(|db, data| app_with_actor(db.pool.clone(), data, actor))
    }

    fn build(router: impl FnOnce(&TestDb, PathBuf) -> Router) -> Self {
        let db = TestDb::new();
        let ids = seed_sample(&mut db.pool.get().unwrap());
        let data = tempfile::tempdir().unwrap();
        let server = TestServer::new(router(&db, data.path().to_path_buf()));
        Self {
            server,
            db,
            data,
            ids,
        }
    }

    /// A second router over the same database and data dir, for requests
    /// axum-test cannot shape (a streamed body, a lying `Content-Length`).
    fn router(&self) -> Router {
        app(self.db.pool.clone(), self.data.path().to_path_buf())
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

    async fn upload(&self, entity_id: &str, form: MultipartForm) -> TestResponse {
        self.server
            .post(&format!("/api/upload/{entity_id}"))
            .multipart(form)
            .await
    }

    async fn graphql(&self, query: &str, variables: Value) -> Value {
        let response = self
            .server
            .post("/graphql")
            .json(&json!({ "query": query, "variables": variables }))
            .await;
        response.assert_status_ok();
        response.json()
    }

    /// The attachments of `entity_id` as GraphQL reports them.
    async fn attachments(&self, entity_id: &str) -> Vec<Value> {
        let body = self
            .graphql(
                "query($id: ID!) { entity(id: $id) { attachments {
                    id kind primary title mimeType sizeBytes url thumbnailUrl
                } } }",
                json!({ "id": entity_id }),
            )
            .await;
        body["data"]["entity"]["attachments"]
            .as_array()
            .unwrap()
            .clone()
    }
}

/// A form holding `bytes` as the `file` field, named `photo.jpg`.
fn photo_form(bytes: Vec<u8>) -> MultipartForm {
    MultipartForm::new().add_part(
        "file",
        Part::bytes(bytes)
            .file_name("photo.jpg")
            .mime_type("image/jpeg"),
    )
}

/// The `{ "error": … }` message of a JSON error response.
fn error_message(response: &TestResponse) -> String {
    let body: Value = response.json();
    body["error"]
        .as_str()
        .unwrap_or_else(|| panic!("expected a JSON error body: {body}"))
        .to_owned()
}

fn primaries(attachments: &[Value]) -> Vec<&str> {
    attachments
        .iter()
        .filter(|a| a["primary"] == true)
        .map(|a| a["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn uploads_a_jpeg_and_returns_the_attachment_json_with_thumbnails_at_all_sizes() {
    let f = Fixture::new();
    let bytes = jpeg(1600, 1200);

    let response = f.upload(&f.ids.screws, photo_form(bytes.clone())).await;

    response.assert_status(StatusCode::CREATED);
    let body: Value = response.json();
    let id = body["id"].as_str().unwrap();
    assert_eq!(body["kind"], "PHOTO");
    assert_eq!(body["primary"], true);
    assert_eq!(body["title"], "photo.jpg");
    assert_eq!(body["mimeType"], "image/jpeg");
    assert_eq!(body["sizeBytes"], bytes.len());
    assert!(
        body["url"]
            .as_str()
            .unwrap()
            .starts_with(&format!("/attachments/{id}?v=")),
        "{body}"
    );
    assert!(body["thumbnailUrl"].is_string(), "{body}");
    assert_eq!(f.originals().len(), 1);

    for size in [300, 500, 1200] {
        let thumb = f
            .server
            .get(&format!("/attachments/{id}/thumb/{size}"))
            .await;
        thumb.assert_status_ok();
        assert_eq!(thumb.header("content-type"), "image/webp");
    }
}

#[tokio::test]
async fn first_photo_is_primary_and_graphql_agrees() {
    let f = Fixture::new();

    let body: Value = f.upload(&f.ids.screws, photo_form(jpeg(8, 6))).await.json();

    assert_eq!(body["primary"], true);
    let primary = f
        .graphql(
            "query($id: ID!) { entity(id: $id) { primaryPhoto { id primary } } }",
            json!({ "id": f.ids.screws }),
        )
        .await;
    assert_eq!(
        primary["data"]["entity"]["primaryPhoto"],
        json!({ "id": body["id"], "primary": true })
    );
}

#[tokio::test]
async fn primary_true_takes_over() {
    let f = Fixture::new();
    let first: Value = f.upload(&f.ids.screws, photo_form(jpeg(8, 6))).await.json();
    let later: Value = f
        .upload(
            &f.ids.screws,
            photo_form(jpeg(9, 6)).add_text("primary", "false"),
        )
        .await
        .json();
    assert_eq!(later["primary"], false);

    let second: Value = f
        .upload(
            &f.ids.screws,
            photo_form(jpeg(10, 6)).add_text("primary", "true"),
        )
        .await
        .json();

    assert_eq!(second["primary"], true);
    let attachments = f.attachments(&f.ids.screws).await;
    assert_eq!(attachments.len(), 3);
    assert_eq!(primaries(&attachments), [second["id"].as_str().unwrap()]);
    assert_ne!(first["id"], second["id"]);
}

#[tokio::test]
async fn rejects_html_as_415_and_stores_nothing() {
    let f = Fixture::new();
    let html = b"<!doctype html><html><script>alert(1)</script></html>".to_vec();

    let response = f.upload(&f.ids.screws, photo_form(html)).await;

    response.assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(!error_message(&response).is_empty());
    assert_eq!(f.originals(), Vec::<String>::new());
    assert!(f.attachments(&f.ids.screws).await.is_empty());
}

/// The opening of a multipart body: a `file` field's headers and a JPEG
/// signature, followed by `len` bytes of padding and no closing boundary.
fn file_part(len: u64) -> impl AsyncRead + Send + 'static {
    let mut head = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; \
         filename=\"big.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n"
    )
    .into_bytes();
    head.extend_from_slice(&[0xFF, 0xD8, 0xFF]);
    Cursor::new(head).chain(io::repeat(0).take(len))
}

/// An upload request whose body is `body`, streamed in 64 KiB chunks with no
/// `Content-Length`.
fn streamed_request(entity_id: &str, body: impl AsyncRead + Send + 'static) -> Request<Body> {
    Request::post(format!("/api/upload/{entity_id}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from_stream(ReaderStream::with_capacity(
            body,
            64 * 1024,
        )))
        .unwrap()
}

/// A complete upload of a JPEG header and `len` bytes of padding, streamed
/// without `Content-Length`, so the limit trips mid-stream after the temp
/// file exists.
fn streamed_upload(entity_id: &str, len: u64) -> Request<Body> {
    let tail = format!("\r\n--{BOUNDARY}--\r\n").into_bytes();
    streamed_request(entity_id, file_part(len).chain(Cursor::new(tail)))
}

/// A body that records being polled and never yields: a handler that reads
/// it hangs, and the flag says it tried.
struct Untouchable(Arc<AtomicBool>);

impl AsyncRead for Untouchable {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.0.store(true, Ordering::SeqCst);
        Poll::Pending
    }
}

/// A client that goes away: every read fails.
struct Disconnected;

impl AsyncRead for Disconnected {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(
            ErrorKind::ConnectionReset,
            "client went away",
        )))
    }
}

fn read_only() -> Actor {
    Actor::User {
        id: "u1".to_owned(),
        role: Role::ReadOnly,
    }
}

async fn json_body(response: Response) -> Value {
    let bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("not JSON ({err}): {bytes:?}"))
}

#[tokio::test]
async fn over_limit_is_413_json_with_no_temp_left() {
    let f = Fixture::new();

    let response = f
        .router()
        .oneshot(streamed_upload(&f.ids.screws, 26 * MIB as u64))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(json_body(response).await["error"].is_string());
    assert_eq!(f.originals(), Vec::<String>::new());
    assert!(f.attachments(&f.ids.screws).await.is_empty());
}

#[tokio::test]
async fn declared_over_limit_length_is_413_json() {
    let f = Fixture::new();
    // The limit layer refuses on the header alone, before any handler runs.
    let request = Request::post(format!("/api/upload/{}", f.ids.screws))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .header(header::CONTENT_LENGTH, 26 * MIB)
        .body(Body::empty())
        .unwrap();

    let response = f.router().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(json_body(response).await["error"].is_string());
}

#[tokio::test]
async fn an_upload_just_under_the_limit_is_accepted() {
    let f = Fixture::new();
    // A real JPEG padded with trailing bytes decoders ignore, just under 25 MiB
    // of body in total, proves the 2 MiB axum default is not in force.
    let mut bytes = jpeg(8, 6);
    bytes.resize(24 * MIB, 0);

    let response = f.upload(&f.ids.screws, photo_form(bytes)).await;

    response.assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn read_only_actor_is_403_before_the_body_is_read() {
    let f = Fixture::with_actor(read_only());
    let polled = Arc::new(AtomicBool::new(false));
    let router = app_with_actor(f.db.pool.clone(), f.data.path().to_path_buf(), read_only());
    let request = streamed_request(&f.ids.screws, Untouchable(Arc::clone(&polled)));

    let response = time::timeout(Duration::from_secs(2), router.oneshot(request))
        .await
        .expect("the handler waited on the body")
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(json_body(response).await["error"].is_string());
    assert!(!polled.load(Ordering::SeqCst), "the body was polled");
    assert_eq!(f.originals(), Vec::<String>::new());
    assert!(f.attachments(&f.ids.screws).await.is_empty());
}

#[tokio::test]
async fn client_abort_mid_file_leaves_nothing() {
    let f = Fixture::new();
    // Most of the limit, so the writer still has work queued when the
    // client goes away and a writer left running would be seen.
    let request = streamed_request(
        &f.ids.screws,
        file_part(20 * MIB as u64).chain(Disconnected),
    );

    let response = f.router().oneshot(request).await.unwrap();

    assert!(response.status().is_client_error(), "{}", response.status());
    assert!(json_body(response).await["error"].is_string());
    assert_eq!(f.originals(), Vec::<String>::new());
    assert!(f.attachments(&f.ids.screws).await.is_empty());
}

#[tokio::test]
async fn the_raised_limit_is_scoped_to_uploads() {
    let f = Fixture::new();
    // Over axum's 2 MiB default, which must still guard every other route.
    let mut query = br#"{"query":"{ summary { totalItems } }","pad":""#.to_vec();
    query.resize(3 * MIB, b' ');
    query.extend_from_slice(b"\"}");

    let response = f
        .server
        .post("/graphql")
        .content_type("application/json")
        .bytes(query.into())
        .await;

    // juniper_axum reports any body failure as 400, so the limit shows in
    // the message rather than the status.
    response.assert_status_bad_request();
    assert!(
        response.text().contains("length limit exceeded"),
        "{}",
        response.text()
    );
}

#[tokio::test]
async fn missing_file_field_is_400() {
    let f = Fixture::new();

    let response = f
        .upload(
            &f.ids.screws,
            MultipartForm::new().add_text("primary", "true"),
        )
        .await;

    response.assert_status_bad_request();
    assert!(!error_message(&response).is_empty());
}

#[tokio::test]
async fn duplicate_file_field_is_400_and_stores_nothing() {
    let f = Fixture::new();
    let form = photo_form(jpeg(8, 6)).add_part("file", Part::bytes(jpeg(9, 6)));

    let response = f.upload(&f.ids.screws, form).await;

    response.assert_status_bad_request();
    assert!(!error_message(&response).is_empty());
    assert_eq!(f.originals(), Vec::<String>::new());
    assert!(f.attachments(&f.ids.screws).await.is_empty());
}

#[tokio::test]
async fn non_multipart_body_is_400_json() {
    let f = Fixture::new();

    let response = f
        .server
        .post(&format!("/api/upload/{}", f.ids.screws))
        .json(&json!({ "file": "nope" }))
        .await;

    response.assert_status_bad_request();
    assert!(!error_message(&response).is_empty());
}

#[tokio::test]
async fn unknown_entity_is_404() {
    let f = Fixture::new();

    let response = f.upload("no-such-entity", photo_form(jpeg(8, 6))).await;

    response.assert_status_not_found();
    assert!(!error_message(&response).is_empty());
    assert_eq!(f.originals(), Vec::<String>::new());
}

#[tokio::test]
async fn graphql_handler_reads_the_actor_extension() {
    let f = Fixture::with_actor(Actor::User {
        id: "u1".to_owned(),
        role: Role::ReadOnly,
    });

    let body = f
        .graphql(
            "mutation($id: ID!) { setPrimaryPhoto(attachmentId: $id) { id } }",
            json!({ "id": f.ids.photo }),
        )
        .await;

    let message = body["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(message.starts_with("Forbidden"), "{body}");
}

#[tokio::test]
async fn urls_in_json_match_graphql() {
    let f = Fixture::new();
    let uploaded: Value = f.upload(&f.ids.screws, photo_form(jpeg(8, 6))).await.json();

    let attachments = f.attachments(&f.ids.screws).await;

    // The upload's JSON is the GraphQL `Attachment` at its default thumbnail size.
    assert_eq!(attachments, [uploaded]);
}
