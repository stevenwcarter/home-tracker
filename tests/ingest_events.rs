//! `GET /api/ingest/batches/{id}/events`: the batch's progress as
//! server-sent events, read incrementally from the live body (axum-test
//! would buffer it whole).

use std::fs;
use std::str;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{self, Body, BodyDataStream};
use axum::http::request::Builder;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use diesel::prelude::*;
use home_tracker::ai::AiState;
use home_tracker::ai::env::AiEnv;
use home_tracker::ai::fake::FakeAiClient;
use home_tracker::db::{ITEM_TYPE_ID, TestDb};
use home_tracker::kinds::IngestBatchStatus;
use home_tracker::models::{IngestBatch, IngestItem, IngestPhoto};
use home_tracker::routes::app_with_ai;
use home_tracker::schema::ingest_batches;
use home_tracker::svc::attachment;
use home_tracker::svc::fixtures::{self, jpeg, seed_sample};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::time;
use tokio_stream::StreamExt;
use tower::ServiceExt;

/// How long a test waits for the next event or the end of a stream.
const EVENT_BOUND: Duration = Duration::from_secs(10);

/// One parsed server-sent event.
#[derive(Debug, PartialEq)]
struct Sse {
    event: String,
    data: Value,
}

/// A live event stream, parsed as its chunks arrive.
struct EventReader {
    body: BodyDataStream,
    buffer: String,
}

impl EventReader {
    fn new(response: Response) -> Self {
        Self {
            body: response.into_body().into_data_stream(),
            buffer: String::new(),
        }
    }

    /// The next event (comments skipped), or `None` once the stream ends.
    async fn next(&mut self) -> Option<Sse> {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..end + 2).collect();
                match parse_block(&block) {
                    Some(event) => return Some(event),
                    None => continue,
                }
            }
            let chunk = time::timeout(EVENT_BOUND, self.body.next())
                .await
                .expect("no event or end of stream in time")?
                .expect("the body failed");
            self.buffer.push_str(str::from_utf8(&chunk).unwrap());
        }
    }
}

/// An SSE block's event name and JSON data; `None` for a comment-only block.
fn parse_block(block: &str) -> Option<Sse> {
    let mut event = None;
    let mut data = None;
    for line in block.lines() {
        if let Some(name) = line.strip_prefix("event:") {
            event = Some(name.trim().to_owned());
        } else if let Some(json) = line.strip_prefix("data:") {
            data = Some(serde_json::from_str(json.trim()).unwrap());
        }
    }
    Some(Sse {
        event: event?,
        data: data.expect("an event carries data"),
    })
}

/// A seeded app whose model is a fake with an API key configured.
struct Fixture {
    app: Router,
    fake: Arc<FakeAiClient>,
    db: TestDb,
    data: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let db = TestDb::new();
        seed_sample(&mut db.pool.get().unwrap());
        let data = tempfile::tempdir().unwrap();
        let fake = Arc::new(FakeAiClient::new());
        let ai = AiState {
            env: AiEnv {
                api_key: Some("test-key".to_owned()),
                base_url: None,
            },
            client: Arc::clone(&fake) as _,
        };
        let app = app_with_ai(db.pool.clone(), data.path().to_path_buf(), Arc::new(ai));
        Self {
            app,
            fake,
            db,
            data,
        }
    }

    /// A collecting batch with one item holding one JPEG, its original on disk.
    fn collecting(&self) -> (IngestBatch, IngestItem, IngestPhoto) {
        let mut conn = self.db.pool.get().unwrap();
        let (batch, item) = fixtures::ingest_batch(&mut conn, None);
        let bytes = jpeg(64, 48);
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let path = attachment::original_path(self.data.path(), &sha256);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        let photo = fixtures::ingest_photo(&mut conn, &item.id, &sha256, "image/jpeg");
        (batch, item, photo)
    }

    async fn request(&self, request: Request<Body>) -> Response {
        self.app.clone().oneshot(request).await.unwrap()
    }

    async fn events(&self, batch_id: &str) -> Response {
        self.request(events_request(batch_id).body(Body::empty()).unwrap())
            .await
    }

    /// Runs a GraphQL document that must succeed.
    async fn graphql(&self, query: &str, variables: Value) {
        let request = Request::post("/graphql")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({ "query": query, "variables": variables }).to_string(),
            ))
            .unwrap();
        let response = self.request(request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let answer: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            answer.get("errors").is_none(),
            "unexpected errors: {answer}"
        );
    }
}

fn events_request(batch_id: &str) -> Builder {
    Request::get(format!("/api/ingest/batches/{batch_id}/events"))
}

fn event(name: &str, id: &str) -> Sse {
    Sse {
        event: name.to_owned(),
        data: json!({ "id": id }),
    }
}

#[tokio::test]
async fn events_stream_photo_item_and_batch_and_ends_on_done() {
    let f = Fixture::new();
    let (batch, item, photo) = f.collecting();
    f.fake.push(Ok(json!({
        "kind": "photo",
        "summary": "A wireless mouse",
        "text": null,
        "details": {},
    })
    .to_string()));
    f.fake.push(Ok(json!({ "name": "Mouse" }).to_string()));

    let response = f.events(&batch.id).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut stream = EventReader::new(response);
    f.graphql(
        "mutation($id: ID!) { submitIngestBatch(id: $id) { id } }",
        json!({ "id": batch.id }),
    )
    .await;

    assert_eq!(stream.next().await, Some(event("photo", &photo.id)));
    assert_eq!(stream.next().await, Some(event("item", &item.id)));
    assert_eq!(stream.next().await, Some(event("batch", &batch.id)));

    f.graphql(
        "mutation($id: ID!, $input: EntityInput!) {
            acceptIngestItem(id: $id, input: $input, photoKinds: []) { id }
        }",
        json!({ "id": item.id, "input": { "name": "Mouse", "entityTypeId": ITEM_TYPE_ID } }),
    )
    .await;

    assert_eq!(stream.next().await, None, "accepting the last item ends it");
}

#[tokio::test]
async fn unknown_batch_is_404() {
    let f = Fixture::new();

    let response = f.events("missing").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_done_batch_yields_one_batch_event_then_ends() {
    let f = Fixture::new();
    let (batch, _, _) = f.collecting();
    diesel::update(ingest_batches::table.find(&batch.id))
        .set(ingest_batches::status.eq(IngestBatchStatus::Done))
        .execute(&mut f.db.pool.get().unwrap())
        .unwrap();

    let response = f.events(&batch.id).await;

    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = EventReader::new(response);
    assert_eq!(stream.next().await, Some(event("batch", &batch.id)));
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn stream_is_not_compressed() {
    let f = Fixture::new();
    let (batch, _, _) = f.collecting();

    let response = f
        .request(
            events_request(&batch.id)
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    assert!(
        response.headers().get(header::CONTENT_ENCODING).is_none(),
        "{:?}",
        response.headers()
    );
}
