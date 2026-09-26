//! The AI ingest surface through `/graphql`: the whole collect, submit,
//! review and accept flow against a scripted model, skip, retry, resuming
//! open batches, the refusals, and the write gate on every ingest mutation.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::http::StatusCode;
use axum_test::TestServer;
use axum_test::multipart::{MultipartForm, Part};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use home_tracker::ai::AiState;
use home_tracker::ai::client::{AiClient, AiError, ChatRequest, ChatResponse, ContentPart};
use home_tracker::ai::env::AiEnv;
use home_tracker::ai::fake::FakeAiClient;
use home_tracker::db::{ITEM_TYPE_ID, TestDb};
use home_tracker::graphql::context::{Actor, Role};
use home_tracker::kinds::{IngestBatchStatus, IngestItemStatus};
use home_tracker::models::{Entity, IngestBatch, IngestItem, IngestPhoto};
use home_tracker::routes::{app_with_actor, app_with_ai};
use home_tracker::schema::{entities, ingest_batches, ingest_items, ingest_photos};
use home_tracker::svc::ai_settings::AiConfig;
use home_tracker::svc::fixtures::{self, SampleIds, jpeg, seed_sample};
use home_tracker::svc::{attachment, ingest};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time;

/// Everything the review screen reads of a batch.
const BATCH: &str = "query($id: ID!) { ingestBatch(id: $id) {
    id parentId status createdAt updatedAt
    items {
        id position status error entityId
        suggestion {
            name manufacturer quantity purchaseDate purchasePriceCents tagNames
            confidence reasoning
        }
        photos {
            id position status error title mimeType sizeBytes url thumbnailUrl
            suggestedKind summary text
        }
    }
} }";

const CREATE: &str = "mutation($parentId: ID) {
    createIngestBatch(parentId: $parentId) { id parentId status items { id status } }
}";

const SUBMIT: &str = "mutation($id: ID!) { submitIngestBatch(id: $id) { id status items { id } } }";

const ACCEPT: &str = "mutation($id: ID!, $input: EntityInput!, $kinds: [IngestPhotoKindInput!]!) {
    acceptIngestItem(id: $id, input: $input, photoKinds: $kinds) {
        id name manufacturer parentId
        attachments { kind primary title }
        primaryPhoto { title }
    }
}";

const SKIP: &str = "mutation($id: ID!) { skipIngestItem(id: $id) { id status photos { id } } }";

const RETRY: &str = "mutation($id: ID!) { retryIngestItem(id: $id) { id status error } }";

/// How long a test waits for the runner, which answers from the fake at once.
const RUNNER_BOUND: Duration = Duration::from_secs(10);

/// A seeded server with an API key configured, whose model is `fake`
/// unless the test brings its own client.
struct Fixture {
    server: TestServer,
    /// The scripted model; left empty when the test brings its own client.
    fake: Arc<FakeAiClient>,
    db: TestDb,
    data: TempDir,
    ids: SampleIds,
}

impl Fixture {
    fn new() -> Self {
        let fake = Arc::new(FakeAiClient::new());
        Self::build(Arc::clone(&fake) as _, fake)
    }

    /// A fixture whose model is `client`.
    fn with_client(client: Arc<dyn AiClient>) -> Self {
        Self::build(client, Arc::new(FakeAiClient::new()))
    }

    fn build(client: Arc<dyn AiClient>, fake: Arc<FakeAiClient>) -> Self {
        let db = TestDb::new();
        let ids = seed_sample(&mut db.pool.get().unwrap());
        let data = tempfile::tempdir().unwrap();
        let ai = AiState {
            env: AiEnv {
                api_key: Some("test-key".to_owned()),
                base_url: None,
            },
            client,
        };
        let server = TestServer::new(app_with_ai(
            db.pool.clone(),
            data.path().to_path_buf(),
            Arc::new(ai),
        ));
        Self {
            server,
            fake,
            db,
            data,
            ids,
        }
    }

    /// The whole response body of `query` with `variables`.
    async fn post(&self, query: &str, variables: Value) -> Value {
        post(&self.server, query, variables).await
    }

    /// The `data` of a query that must succeed.
    async fn data(&self, query: &str, variables: Value) -> Value {
        let body = self.post(query, variables).await;
        assert!(body.get("errors").is_none(), "unexpected errors: {body}");
        body["data"].clone()
    }

    /// The single error message of a query that must fail.
    async fn error(&self, query: &str, variables: Value) -> String {
        only_error(&self.post(query, variables).await)
    }

    async fn batch(&self, id: &str) -> Value {
        self.data(BATCH, json!({ "id": id })).await["ingestBatch"].clone()
    }

    /// Polls batch `id` until `ready` holds for it.
    async fn wait_for(&self, id: &str, ready: impl Fn(&Value) -> bool) -> Value {
        time::timeout(RUNNER_BOUND, async {
            loop {
                let batch = self.batch(id).await;
                if ready(&batch) {
                    return batch;
                }
                time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("batch {id} did not get there in {RUNNER_BOUND:?}"))
    }

    async fn wait_for_status(&self, id: &str, status: &str) -> Value {
        self.wait_for(id, |batch| batch["status"] == status).await
    }

    /// A new batch under `parent`: its id and its first item's id.
    async fn create(&self, parent: Option<&str>) -> (String, String) {
        let batch =
            self.data(CREATE, json!({ "parentId": parent })).await["createIngestBatch"].clone();
        (
            batch["id"].as_str().unwrap().to_owned(),
            batch["items"][0]["id"].as_str().unwrap().to_owned(),
        )
    }

    /// Stages `bytes` for item `item_id` through the HTTP upload.
    async fn stage(&self, item_id: &str, bytes: Vec<u8>, name: &str) -> Value {
        let form = MultipartForm::new().add_part(
            "file",
            Part::bytes(bytes).file_name(name).mime_type("image/jpeg"),
        );
        let response = self
            .server
            .post(&format!("/api/ingest/items/{item_id}/photos"))
            .multipart(form)
            .await;
        response.assert_status(StatusCode::CREATED);
        response.json()
    }

    /// A batch of one item with one staged photo, submitted; its ids.
    async fn submitted_one(&self) -> (String, String) {
        let (batch, item) = self.create(None).await;
        self.stage(&item, jpeg(64, 48), "photo.jpg").await;
        self.data(SUBMIT, json!({ "id": batch })).await;
        (batch, item)
    }

    /// Queues the answers that describe one photo as a receipt.
    fn push_description(&self) {
        self.fake.push(Ok(json!({
            "kind": "receipt",
            "summary": "Amazon receipt for a Logitech mouse",
            "text": "Logitech MX Master 3S 99.99",
            "details": { "brand": "Logitech" },
        })
        .to_string()));
    }

    /// Queues a synthesis answer suggesting `name`.
    fn push_suggestion(&self, name: &str) {
        self.fake.push(Ok(json!({
            "name": name,
            "manufacturer": "Logitech",
            "quantity": 1,
            "purchase_date": "2024-03-12",
            "purchase_price_cents": 9999,
            "tag_names": ["electronics", "Unknown"],
            "confidence": "high",
            "reasoning": "Printed on the receipt",
        })
        .to_string()));
    }

    fn conn(&self) -> PooledConnection<ConnectionManager<SqliteConnection>> {
        self.db.pool.get().unwrap()
    }

    /// The ids of the open batches started from `parent`, as listed.
    async fn open_batches(&self, parent: Option<&str>) -> Vec<String> {
        let data = self
            .data(
                "query($p: ID) { openIngestBatches(parentId: $p) { id } }",
                json!({ "p": parent }),
            )
            .await;
        data["openIngestBatches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| str_of(&b["id"], "id").to_owned())
            .collect()
    }

    /// How many originals are stored.
    fn originals(&self) -> usize {
        fs::read_dir(attachment::originals_dir(self.data.path())).map_or(0, |dir| dir.count())
    }
}

async fn post(server: &TestServer, query: &str, variables: Value) -> Value {
    let response = server
        .post("/graphql")
        .json(&json!({ "query": query, "variables": variables }))
        .await;
    response.assert_status_ok();
    response.json()
}

/// The message of `body`'s one error.
fn only_error(body: &Value) -> String {
    let errors = body["errors"]
        .as_array()
        .unwrap_or_else(|| panic!("expected errors: {body}"));
    assert_eq!(errors.len(), 1, "{body}");
    errors[0]["message"].as_str().unwrap().to_owned()
}

fn str_of<'a>(value: &'a Value, what: &str) -> &'a str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{what} is not a string: {value}"))
}

/// Answers every vision call with a photo description and every synthesis
/// call with a suggestion, whatever order the runner makes them in.
struct ByKind;

#[async_trait]
impl AiClient for ByKind {
    async fn chat(&self, _: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        let vision = request
            .messages
            .iter()
            .flat_map(|message| &message.content)
            .any(|part| matches!(part, ContentPart::ImageUrl { .. }));
        let content = if vision {
            json!({
                "kind": "photo",
                "summary": "A cordless drill",
                "text": null,
                "details": {},
            })
        } else {
            json!({
                "name": "Cordless drill",
                "quantity": 1,
                "tag_names": [],
                "confidence": "high",
            })
        };
        Ok(ChatResponse {
            content: content.to_string(),
            usage: None,
            model: request.model,
        })
    }
}

/// Every ingest row and every entity, in a stable order.
#[derive(Debug, PartialEq)]
struct Snapshot {
    batches: Vec<IngestBatch>,
    items: Vec<IngestItem>,
    photos: Vec<IngestPhoto>,
    entities: Vec<Entity>,
}

fn snapshot(conn: &mut SqliteConnection) -> Snapshot {
    Snapshot {
        batches: ingest_batches::table
            .order(ingest_batches::id)
            .select(IngestBatch::as_select())
            .load(conn)
            .unwrap(),
        items: ingest_items::table
            .order(ingest_items::id)
            .select(IngestItem::as_select())
            .load(conn)
            .unwrap(),
        photos: ingest_photos::table
            .order(ingest_photos::id)
            .select(IngestPhoto::as_select())
            .load(conn)
            .unwrap(),
        entities: entities::table
            .order(entities::id)
            .select(Entity::as_select())
            .load(conn)
            .unwrap(),
    }
}

#[tokio::test]
async fn create_add_upload_submit_review_accept_flow() {
    let f = Fixture::new();
    let (batch_id, item_id) = f.create(Some(&f.ids.garage)).await;
    let created = f.batch(&batch_id).await;
    assert_eq!(created["status"], "COLLECTING");
    assert_eq!(created["parentId"], f.ids.garage.as_str());
    let added = f
        .data(
            "mutation($id: ID!) { addIngestItem(batchId: $id) { id status photos { id } } }",
            json!({ "id": batch_id }),
        )
        .await["addIngestItem"]
        .clone();
    assert_eq!(added["status"], "COLLECTING");
    let first = f.stage(&item_id, jpeg(640, 480), "receipt.jpg").await;
    let second = f.stage(&item_id, jpeg(480, 640), "mouse.jpg").await;
    let (first_id, second_id) = (str_of(&first["id"], "id"), str_of(&second["id"], "id"));
    f.push_description();
    f.push_description();
    f.push_suggestion("Suggested mouse");

    let submitted = f.data(SUBMIT, json!({ "id": batch_id })).await["submitIngestBatch"].clone();

    assert_eq!(submitted["status"], "PROCESSING");
    assert_eq!(
        submitted["items"],
        json!([{ "id": item_id }]),
        "the empty added item is dropped"
    );
    let reviewing = f.wait_for_status(&batch_id, "REVIEWING").await;
    let item = &reviewing["items"][0];
    assert_eq!(item["status"], "READY");
    assert_eq!(item["error"], Value::Null);
    assert_eq!(
        item["suggestion"],
        json!({
            "name": "Suggested mouse",
            "manufacturer": "Logitech",
            "quantity": 1.0,
            "purchaseDate": "2024-03-12",
            "purchasePriceCents": 9999,
            "tagNames": ["Electronics"],
            "confidence": "high",
            "reasoning": "Printed on the receipt",
        })
    );
    let photos = item["photos"].as_array().unwrap();
    assert_eq!(photos.len(), 2);
    for photo in photos {
        assert_eq!(photo["status"], "DESCRIBED");
        assert_eq!(photo["suggestedKind"], "RECEIPT");
        assert_eq!(photo["summary"], "Amazon receipt for a Logitech mouse");
        assert_eq!(photo["text"], "Logitech MX Master 3S 99.99");
        let id = str_of(&photo["id"], "id");
        assert!(
            str_of(&photo["url"], "url").starts_with(&format!("/ingest/photos/{id}?v=")),
            "{photo}"
        );
        assert!(
            str_of(&photo["thumbnailUrl"], "thumbnailUrl")
                .starts_with(&format!("/ingest/photos/{id}/thumb/500?v=")),
            "{photo}"
        );
    }
    assert_eq!(f.originals(), 2);

    // The user renames the item, files it elsewhere, and calls the second
    // photo a photo; the first keeps the kind the model suggested.
    let accepted = f
        .data(
            ACCEPT,
            json!({
                "id": item_id,
                "input": {
                    "name": "My mouse",
                    "entityTypeId": ITEM_TYPE_ID,
                    "parentId": f.ids.tote_a,
                },
                "kinds": [{ "photoId": second_id, "kind": "PHOTO" }],
            }),
        )
        .await["acceptIngestItem"]
        .clone();

    assert_eq!(accepted["name"], "My mouse");
    assert_eq!(
        accepted["manufacturer"],
        Value::Null,
        "the user's values, not the suggestion"
    );
    assert_eq!(accepted["parentId"], f.ids.tote_a.as_str());
    let mut attachments = accepted["attachments"].as_array().unwrap().clone();
    attachments.sort_by_key(|a| a["title"].as_str().unwrap().to_owned());
    assert_eq!(
        attachments,
        [
            json!({ "kind": "PHOTO", "primary": true, "title": "mouse.jpg" }),
            json!({ "kind": "RECEIPT", "primary": false, "title": "receipt.jpg" }),
        ]
    );
    assert_eq!(accepted["primaryPhoto"]["title"], "mouse.jpg");
    let done = f.batch(&batch_id).await;
    assert_eq!(done["status"], "DONE");
    assert_eq!(done["items"][0]["status"], "ACCEPTED");
    assert_eq!(done["items"][0]["entityId"], accepted["id"]);
    assert_eq!(done["items"][0]["photos"], json!([]));
    assert!(
        ingest::get_photo(&mut f.conn(), first_id)
            .unwrap()
            .is_none(),
        "the staged rows are gone"
    );
    assert_eq!(
        f.originals(),
        2,
        "the files stay, shared by the attachments"
    );
}

#[tokio::test]
async fn two_items_two_photos_accept_and_skip_show_the_new_item_in_its_location() {
    let f = Fixture::with_client(Arc::new(ByKind));
    let (batch_id, first_item) = f.create(Some(&f.ids.garage)).await;
    let second_item = f
        .data(
            "mutation($id: ID!) { addIngestItem(batchId: $id) { id } }",
            json!({ "id": batch_id }),
        )
        .await["addIngestItem"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let front = f.stage(&first_item, jpeg(64, 48), "front.jpg").await;
    let receipt = f.stage(&first_item, jpeg(48, 64), "receipt.jpg").await;
    f.stage(&second_item, jpeg(40, 30), "other-1.jpg").await;
    f.stage(&second_item, jpeg(30, 40), "other-2.jpg").await;

    f.data(SUBMIT, json!({ "id": batch_id })).await;
    let reviewing = f.wait_for_status(&batch_id, "REVIEWING").await;

    let statuses: Vec<&Value> = reviewing["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| &item["status"])
        .collect();
    assert_eq!(statuses, ["READY", "READY"], "{reviewing}");
    assert_eq!(f.originals(), 4);
    let accepted = f
        .data(
            ACCEPT,
            json!({
                "id": first_item,
                "input": {
                    "name": "My drill",
                    "entityTypeId": ITEM_TYPE_ID,
                    "parentId": f.ids.garage,
                },
                "kinds": [
                    { "photoId": front["id"], "kind": "PHOTO" },
                    { "photoId": receipt["id"], "kind": "RECEIPT" },
                ],
            }),
        )
        .await["acceptIngestItem"]
        .clone();
    assert_eq!(
        f.originals(),
        4,
        "accepting moves the files to attachments, it copies none"
    );
    f.data(SKIP, json!({ "id": second_item })).await;

    assert_eq!(f.batch(&batch_id).await["status"], "DONE");
    assert_eq!(f.originals(), 2, "the skipped item's files are gone");
    let garage = f
        .data(
            "query($id: ID!) { entity(id: $id) { items {
                id name primaryPhoto { title } attachments { kind title }
            } } }",
            json!({ "id": f.ids.garage }),
        )
        .await["entity"]
        .clone();
    let drill = garage["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == accepted["id"])
        .unwrap_or_else(|| panic!("the new item is not in the garage: {garage}"));
    assert_eq!(drill["name"], "My drill");
    assert_eq!(drill["primaryPhoto"]["title"], "front.jpg");
    let mut attachments = drill["attachments"].as_array().unwrap().clone();
    attachments.sort_by_key(|a| a["title"].as_str().unwrap().to_owned());
    assert_eq!(
        attachments,
        [
            json!({ "kind": "PHOTO", "title": "front.jpg" }),
            json!({ "kind": "RECEIPT", "title": "receipt.jpg" }),
        ]
    );
}

#[tokio::test]
async fn skip_then_done() {
    let f = Fixture::new();
    f.push_description();
    f.push_suggestion("Mouse");
    let (batch_id, item_id) = f.submitted_one().await;
    f.wait_for_status(&batch_id, "REVIEWING").await;

    let skipped = f.data(SKIP, json!({ "id": item_id })).await["skipIngestItem"].clone();

    assert_eq!(skipped["status"], "SKIPPED");
    assert_eq!(skipped["photos"], json!([]));
    assert_eq!(f.batch(&batch_id).await["status"], "DONE");
    assert_eq!(f.originals(), 0, "nothing else shared the photo");
    assert_eq!(
        f.error(SKIP, json!({ "id": item_id })).await,
        "only an item that is ready or failed can be accepted or skipped"
    );
}

#[tokio::test]
async fn retry_after_a_failed_item() {
    let f = Fixture::new();
    // Nothing queued: the fake refuses the vision call, so the item fails.
    let (batch_id, item_id) = f.submitted_one().await;
    let reviewing = f.wait_for_status(&batch_id, "REVIEWING").await;
    let failed = &reviewing["items"][0];
    assert_eq!(failed["status"], "FAILED");
    assert!(failed["error"].is_string(), "{failed}");
    assert_eq!(failed["photos"][0]["status"], "FAILED");
    f.push_description();
    f.push_suggestion("Mouse");

    let retried = f.data(RETRY, json!({ "id": item_id })).await["retryIngestItem"].clone();

    assert_eq!(
        retried,
        json!({ "id": item_id, "status": "QUEUED", "error": null })
    );
    let ready = f
        .wait_for(&batch_id, |batch| batch["items"][0]["status"] == "READY")
        .await;
    assert_eq!(ready["items"][0]["suggestion"]["name"], "Mouse");
    assert_eq!(ready["items"][0]["photos"][0]["status"], "DESCRIBED");
    let reviewing = f.wait_for_status(&batch_id, "REVIEWING").await;
    assert_eq!(reviewing["items"][0]["status"], "READY");

    let (collecting, open) = f.create(None).await;
    assert_eq!(
        f.error(RETRY, json!({ "id": open })).await,
        "only an item that is ready or failed can be retried"
    );
    assert_eq!(f.batch(&collecting).await["status"], "COLLECTING");
    assert_eq!(
        f.error(RETRY, json!({ "id": "missing" })).await,
        "ingest item not found"
    );
}

#[tokio::test]
async fn open_batches_lists_only_unfinished_ones_for_the_parent() {
    let f = Fixture::new();
    let (older, _) = f.create(Some(&f.ids.garage)).await;
    let (finished, _) = f.create(Some(&f.ids.garage)).await;
    let (newer, _) = f.create(Some(&f.ids.garage)).await;
    let (elsewhere, _) = f.create(Some(&f.ids.house)).await;
    let (parentless, _) = f.create(None).await;
    // Every status but done counts as open.
    for (id, status) in [
        (&older, IngestBatchStatus::Processing),
        (&finished, IngestBatchStatus::Done),
        (&newer, IngestBatchStatus::Reviewing),
    ] {
        diesel::update(ingest_batches::table.find(id))
            .set(ingest_batches::status.eq(status))
            .execute(&mut f.conn())
            .unwrap();
    }

    assert_eq!(f.open_batches(Some(&f.ids.garage)).await, [newer, older]);
    assert_eq!(f.open_batches(Some(&f.ids.house)).await, [elsewhere]);
    assert_eq!(f.open_batches(None).await, [parentless]);
    assert!(f.open_batches(Some(&f.ids.attic)).await.is_empty());
}

#[tokio::test]
async fn mutations_require_write() {
    let db = TestDb::new();
    let data = tempfile::tempdir().unwrap();
    let mut conn = db.pool.get().unwrap();
    let ids = seed_sample(&mut conn);
    let (batch, item) = fixtures::ingest_batch(&mut conn, Some(&ids.garage));
    let photo = fixtures::ingest_photo(&mut conn, &item.id, &"cc".repeat(32), "image/jpeg");
    let before = snapshot(&mut conn);
    let server = TestServer::new(app_with_actor(
        db.pool.clone(),
        data.path().to_path_buf(),
        Actor::User {
            id: "u1".to_owned(),
            role: Role::ReadOnly,
        },
    ));
    let (b, i, p) = (&batch.id, &item.id, &photo.id);
    let documents = [
        format!(
            r#"mutation {{ createIngestBatch(parentId: "{}") {{ id }} }}"#,
            ids.garage
        ),
        format!(r#"mutation {{ addIngestItem(batchId: "{b}") {{ id }} }}"#),
        format!(r#"mutation {{ removeIngestItem(id: "{i}") }}"#),
        format!(r#"mutation {{ removeIngestPhoto(id: "{p}") }}"#),
        format!(r#"mutation {{ submitIngestBatch(id: "{b}") {{ id }} }}"#),
        format!(r#"mutation {{ retryIngestItem(id: "{i}") {{ id }} }}"#),
        format!(
            r#"mutation {{ acceptIngestItem(id: "{i}", input: {{ name: "Mouse", entityTypeId: "{ITEM_TYPE_ID}" }}, photoKinds: [{{ photoId: "{p}", kind: PHOTO }}]) {{ id }} }}"#
        ),
        format!(r#"mutation {{ skipIngestItem(id: "{i}") {{ id }} }}"#),
        format!(r#"mutation {{ deleteIngestBatch(id: "{b}") }}"#),
    ];

    for document in &documents {
        let body = post(&server, document, json!({})).await;
        let message = only_error(&body);
        assert!(message.contains("Forbidden"), "{document}: {body}");
    }

    assert_eq!(snapshot(&mut conn), before);
}

#[tokio::test]
async fn submit_of_an_empty_batch_is_refused() {
    let f = Fixture::new();
    let (batch_id, _) = f.create(None).await;

    assert_eq!(
        f.error(SUBMIT, json!({ "id": batch_id })).await,
        "add at least one photo before submitting"
    );
    let batch = f.batch(&batch_id).await;
    assert_eq!(batch["status"], "COLLECTING");
    assert_eq!(batch["items"].as_array().unwrap().len(), 1);
    assert!(f.fake.requests().is_empty());
}

#[tokio::test]
async fn accept_of_a_queued_item_is_refused() {
    let f = Fixture::new();
    let (batch, item) = {
        let mut conn = f.conn();
        let (batch, item) = fixtures::ingest_batch(&mut conn, None);
        fixtures::ingest_photo(&mut conn, &item.id, &"cc".repeat(32), "image/jpeg");
        // Submitted without the runner, so the item stays queued.
        ingest::submit(&mut conn, &batch.id).unwrap();
        (batch, item)
    };
    let before = snapshot(&mut f.conn());

    let message = f
        .error(
            ACCEPT,
            json!({
                "id": item.id,
                "input": { "name": "Mouse", "entityTypeId": ITEM_TYPE_ID },
                "kinds": [],
            }),
        )
        .await;

    assert_eq!(
        message,
        "only an item that is ready or failed can be accepted or skipped"
    );
    assert_eq!(snapshot(&mut f.conn()), before);
    assert_eq!(f.batch(&batch.id).await["items"][0]["status"], "QUEUED");
}

#[tokio::test]
async fn accept_with_a_photo_of_another_item_is_refused() {
    let f = Fixture::new();
    let (item, foreign) = {
        let mut conn = f.conn();
        let (batch, item) = fixtures::ingest_batch(&mut conn, None);
        fixtures::ingest_photo(&mut conn, &item.id, &"cc".repeat(32), "image/jpeg");
        let (_, other) = fixtures::ingest_batch(&mut conn, None);
        let foreign = fixtures::ingest_photo(&mut conn, &other.id, &"dd".repeat(32), "image/jpeg");
        ingest::submit(&mut conn, &batch.id).unwrap();
        ingest::set_item_status(&mut conn, &item.id, IngestItemStatus::Ready, None).unwrap();
        (item, foreign)
    };
    let before = snapshot(&mut f.conn());

    let message = f
        .error(
            ACCEPT,
            json!({
                "id": item.id,
                "input": { "name": "Mouse", "entityTypeId": ITEM_TYPE_ID },
                "kinds": [{ "photoId": foreign.id, "kind": "PHOTO" }],
            }),
        )
        .await;

    assert_eq!(message, "photo kinds name a photo of another item");
    assert_eq!(snapshot(&mut f.conn()), before);
}

#[tokio::test]
async fn changing_a_submitted_batch_is_refused() {
    let f = Fixture::new();
    let (batch, photo) = {
        let mut conn = f.conn();
        let (batch, item) = fixtures::ingest_batch(&mut conn, None);
        let photo = fixtures::ingest_photo(&mut conn, &item.id, &"cc".repeat(32), "image/jpeg");
        ingest::submit(&mut conn, &batch.id).unwrap();
        (batch, photo)
    };
    let submitted = "this batch has been submitted and can no longer change";

    assert_eq!(
        f.error(
            "mutation($id: ID!) { addIngestItem(batchId: $id) { id } }",
            json!({ "id": batch.id }),
        )
        .await,
        submitted
    );
    assert_eq!(
        f.error(
            "mutation($id: ID!) { removeIngestPhoto(id: $id) }",
            json!({ "id": photo.id }),
        )
        .await,
        submitted
    );
    assert_eq!(
        f.batch(&batch.id).await["items"][0]["photos"][0]["id"],
        photo.id.as_str()
    );
}
