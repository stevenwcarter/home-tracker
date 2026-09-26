//! The ingest runner end to end against scripted model clients: parallel
//! description, synthesis, failures that stay local to a photo or an item,
//! retry, the model-call bound, events, and resuming after a restart.

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use home_tracker::ai::AiState;
use home_tracker::ai::client::{
    AiClient, AiError, ChatRequest, ChatResponse, ContentPart, ResponseFormat, Role,
};
use home_tracker::ai::env::AiEnv;
use home_tracker::ai::fake::FakeAiClient;
use home_tracker::ai::prompts::{SYNTHESIS_SYSTEM, VISION_SYSTEM};
use home_tracker::db::TestDb;
use home_tracker::ingest::events::{IngestEvent, IngestEvents};
use home_tracker::ingest::runner::{DESCRIBE_MAX_TOKENS, IngestRunner, SYNTHESIS_MAX_TOKENS};
use home_tracker::kinds::{IngestBatchStatus, IngestItemStatus, IngestPhotoStatus};
use home_tracker::models::{IngestBatch, IngestItem, IngestPhoto};
use home_tracker::svc::ai_settings::AiConfig;
use home_tracker::svc::fixtures::{self, jpeg, seed_sample};
use home_tracker::svc::thumbnail_service::ThumbnailService;
use home_tracker::svc::{attachment, ingest};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::sync::Semaphore;
use tokio::sync::broadcast::Receiver;
use tokio::time;

const NOT_JSON: &str = "The model's answer was not valid JSON";
const DATA_URL_PREFIX: &str = "data:image/webp;base64,";

/// A seeded database, a data dir and a runner over `client`.
struct Fixture {
    db: TestDb,
    data: TempDir,
    runner: Arc<IngestRunner>,
}

impl Fixture {
    /// A runner with an API key configured (through the environment
    /// overrides, so no settings row is needed).
    fn new(client: Arc<dyn AiClient>) -> Self {
        Self::with_env(
            AiEnv {
                api_key: Some("test-key".to_owned()),
                base_url: None,
            },
            client,
        )
    }

    fn with_env(env: AiEnv, client: Arc<dyn AiClient>) -> Self {
        let db = TestDb::new();
        seed_sample(&mut db.pool.get().unwrap());
        let data = tempfile::tempdir().unwrap();
        let thumbnails = ThumbnailService::new(db.pool.clone(), data.path().to_path_buf());
        let runner = IngestRunner::new(
            db.pool.clone(),
            thumbnails,
            Arc::new(AiState { env, client }),
            IngestEvents::new(),
        );
        Self { db, data, runner }
    }

    /// A submitted batch with one item of `photos` distinct JPEG photos,
    /// their originals on disk.
    fn submitted(&self, photos: u32) -> (IngestBatch, IngestItem) {
        let mut conn = self.db.pool.get().unwrap();
        let (batch, item) = fixtures::ingest_batch(&mut conn, None);
        for n in 0..photos {
            let bytes = jpeg(40 + n, 30);
            let sha256 = hex::encode(Sha256::digest(&bytes));
            let path = attachment::original_path(self.data.path(), &sha256);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
            fixtures::ingest_photo(&mut conn, &item.id, &sha256, "image/jpeg");
        }
        let batch = ingest::submit(&mut conn, &batch.id).unwrap();
        (batch, item)
    }

    fn batch(&self, id: &str) -> IngestBatch {
        ingest::get_batch(&mut self.db.pool.get().unwrap(), id)
            .unwrap()
            .unwrap()
    }

    fn item(&self, id: &str) -> IngestItem {
        ingest::get_item(&mut self.db.pool.get().unwrap(), id)
            .unwrap()
            .unwrap()
    }

    fn photos(&self, item_id: &str) -> Vec<IngestPhoto> {
        ingest::photos(&mut self.db.pool.get().unwrap(), item_id).unwrap()
    }

    fn set_item_status(&self, id: &str, status: IngestItemStatus) {
        ingest::set_item_status(&mut self.db.pool.get().unwrap(), id, status, None).unwrap();
    }
}

fn description(kind: &str, summary: &str) -> String {
    json!({
        "kind": kind,
        "summary": summary,
        "text": null,
        "details": { "brand": "Logitech" },
    })
    .to_string()
}

fn suggestion(name: &str) -> String {
    json!({
        "name": name,
        "manufacturer": "Logitech",
        "quantity": 1,
        "purchase_price_cents": 9999,
        "tag_names": [],
        "confidence": "high",
    })
    .to_string()
}

/// The text of `request`'s system message.
fn system_text(request: &ChatRequest) -> &str {
    let system = &request.messages[0];
    assert_eq!(system.role, Role::System);
    match &system.content[..] {
        [ContentPart::Text(text)] => text,
        other => panic!("unexpected system content {other:?}"),
    }
}

/// Every text part of `request`'s user messages, joined.
fn user_text(request: &ChatRequest) -> String {
    request
        .messages
        .iter()
        .filter(|message| message.role == Role::User)
        .flat_map(|message| &message.content)
        .filter_map(|part| match part {
            ContentPart::Text(text) => Some(text.as_str()),
            ContentPart::ImageUrl { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every image URL of `request`.
fn image_urls(request: &ChatRequest) -> Vec<&str> {
    request
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|part| match part {
            ContentPart::ImageUrl { url, .. } => Some(url.as_str()),
            ContentPart::Text(_) => None,
        })
        .collect()
}

fn schema_name(request: &ChatRequest) -> &str {
    match &request.response_format {
        Some(ResponseFormat::JsonSchema { name, .. }) => name,
        other => panic!("unexpected response format {other:?}"),
    }
}

/// How many events of each kind `events` has received so far.
fn drain(events: &mut Receiver<IngestEvent>) -> HashMap<&'static str, usize> {
    let mut counts = HashMap::new();
    while let Ok(event) = events.try_recv() {
        *counts.entry(event.kind.as_str()).or_default() += 1;
    }
    counts
}

fn suggestion_of(item: &IngestItem) -> Value {
    serde_json::from_str(item.suggestion.as_deref().expect("a suggestion")).unwrap()
}

fn errors_of(photos: &[IngestPhoto]) -> Vec<Option<&str>> {
    let mut errors: Vec<Option<&str>> = photos.iter().map(|p| p.error.as_deref()).collect();
    errors.sort_unstable();
    errors
}

#[tokio::test]
async fn describes_every_photo_in_parallel_and_synthesises_the_item() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(2);
    fake.push(Ok(description("receipt", "Amazon receipt for a mouse")));
    fake.push(Ok(description("photo", "A black wireless mouse")));
    fake.push(Ok(suggestion("Logitech MX Master 3S")));
    let mut events = f.runner.events().subscribe(&batch.id);

    f.runner.run_batch(&batch.id).await.unwrap();

    let requests = fake.requests();
    assert_eq!(requests.len(), 3);
    for vision in &requests[..2] {
        assert_eq!(system_text(vision), VISION_SYSTEM);
        let urls = image_urls(vision);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].starts_with(DATA_URL_PREFIX), "{}", &urls[0][..40]);
        assert!(urls[0].len() > DATA_URL_PREFIX.len());
        assert_eq!(schema_name(vision), "photo_description");
        assert_eq!(vision.max_tokens, Some(DESCRIBE_MAX_TOKENS));
        assert!(user_text(vision).ends_with("of 2 of one item."));
    }
    let synthesis = &requests[2];
    assert_eq!(system_text(synthesis), SYNTHESIS_SYSTEM);
    assert!(image_urls(synthesis).is_empty());
    assert_eq!(schema_name(synthesis), "item_suggestion");
    assert_eq!(synthesis.max_tokens, Some(SYNTHESIS_MAX_TOKENS));
    let text = user_text(synthesis);
    for needle in [
        "Amazon receipt for a mouse",
        "A black wireless mouse",
        "USD",
    ] {
        assert!(text.contains(needle), "{needle:?} missing from {text}");
    }

    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Ready);
    assert_eq!(item.error, None);
    let stored = suggestion_of(&item);
    assert_eq!(stored["name"], "Logitech MX Master 3S");
    assert_eq!(stored["purchase_price_cents"], 9999);
    let photos = f.photos(&item.id);
    assert!(
        photos
            .iter()
            .all(|p| p.status == IngestPhotoStatus::Described)
    );
    let mut kinds: Vec<&str> = photos
        .iter()
        .map(|p| p.suggested_kind.unwrap().as_str())
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, ["photo", "receipt"]);
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
    assert_eq!(
        drain(&mut events),
        HashMap::from([("photo", 2), ("item", 1), ("batch", 1)])
    );
}

#[tokio::test]
async fn a_failed_photo_does_not_stop_the_item() {
    // Review Focus 2: an answer that is prose, not the JSON asked for.
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(2);
    fake.push(Ok("I can see a black computer mouse on a desk.".to_owned()));
    fake.push(Ok(description("photo", "A black wireless mouse")));
    fake.push(Ok(suggestion("Mouse")));

    f.runner.run_batch(&batch.id).await.unwrap();

    let photos = f.photos(&item.id);
    assert_eq!(errors_of(&photos), [None, Some(NOT_JSON)]);
    let failed = photos.iter().find(|p| p.error.is_some()).unwrap();
    assert_eq!(failed.status, IngestPhotoStatus::Failed);
    assert_eq!(failed.description, None);
    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Ready);
    assert_eq!(suggestion_of(&item)["name"], "Mouse");
    let synthesis = user_text(&fake.requests()[2]);
    assert!(synthesis.contains("A black wireless mouse"));
    assert!(!synthesis.contains("computer mouse on a desk"));
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

#[tokio::test]
async fn an_item_with_no_described_photo_is_failed() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(2);
    fake.push(Ok("[\"not\", \"an object\"]".to_owned()));
    fake.push(Err(AiError::Timeout));

    f.runner.run_batch(&batch.id).await.unwrap();

    // No synthesis call without a description to synthesise from.
    assert_eq!(fake.requests().len(), 2);
    let photos = f.photos(&item.id);
    assert!(photos.iter().all(|p| p.status == IngestPhotoStatus::Failed));
    assert_eq!(
        errors_of(&photos),
        [
            Some("The model's answer was JSON but not an object"),
            Some("the AI provider did not answer in time"),
        ]
    );
    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Failed);
    assert_eq!(
        item.error.as_deref(),
        Some("None of this item's photos could be described")
    );
    assert_eq!(item.suggestion, None);
    // A failed item is reviewable, so the batch still reaches reviewing.
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

#[tokio::test]
async fn a_bad_synthesis_answer_fails_the_item_readably() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(1);
    fake.push(Ok(description("photo", "A mouse")));
    fake.push(Ok(r#"{"name": "Mouse", "notes": "cut of"#.to_owned()));

    f.runner.run_batch(&batch.id).await.unwrap();

    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Failed);
    assert_eq!(item.error.as_deref(), Some(NOT_JSON));
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

#[tokio::test]
async fn retry_reruns_only_failed_photos_and_the_synthesis() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(2);
    fake.push(Ok("Sorry, I cannot help with that.".to_owned()));
    fake.push(Ok(description("photo", "A black wireless mouse")));
    fake.push(Ok(suggestion("First guess")));
    f.runner.run_batch(&batch.id).await.unwrap();
    assert_eq!(f.item(&item.id).status, IngestItemStatus::Ready);
    assert_eq!(errors_of(&f.photos(&item.id)), [None, Some(NOT_JSON)]);

    // What `retryIngestItem` does: back to queued, then run the item.
    f.set_item_status(&item.id, IngestItemStatus::Queued);
    fake.push(Ok(description("receipt", "Amazon receipt for a mouse")));
    fake.push(Ok(suggestion("Second guess")));
    let mut events = f.runner.events().subscribe(&batch.id);
    f.runner.run_item(&item.id).await.unwrap();

    let requests = fake.requests();
    assert_eq!(requests.len(), 5, "one vision call and one synthesis");
    assert_eq!(image_urls(&requests[3]).len(), 1);
    let synthesis = user_text(&requests[4]);
    assert!(synthesis.contains("A black wireless mouse"));
    assert!(synthesis.contains("Amazon receipt for a mouse"));
    let photos = f.photos(&item.id);
    assert!(
        photos
            .iter()
            .all(|p| p.status == IngestPhotoStatus::Described && p.error.is_none())
    );
    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Ready);
    assert_eq!(suggestion_of(&item)["name"], "Second guess");
    // The batch was already reviewing, so only the photo and item changed.
    assert_eq!(
        drain(&mut events),
        HashMap::from([("photo", 1), ("item", 1)])
    );
}

#[tokio::test]
async fn tag_names_are_filtered_to_existing_tags_case_insensitively() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(1);
    fake.push(Ok(description("photo", "A cordless drill")));
    fake.push(Ok(json!({
        "name": "Drill",
        "tag_names": ["electronics", "Garden", "TOOLS", "Tools"],
    })
    .to_string()));

    f.runner.run_batch(&batch.id).await.unwrap();

    // The model was offered every existing tag by name.
    let synthesis = user_text(&fake.requests()[1]);
    assert!(synthesis.contains("\"Electronics\""), "{synthesis}");
    assert!(synthesis.contains("\"Tools\""), "{synthesis}");
    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Ready);
    assert_eq!(
        suggestion_of(&item)["tag_names"],
        json!(["Electronics", "Tools"])
    );
}

#[tokio::test]
async fn a_not_configured_client_fails_the_item_readably() {
    // No key anywhere: nothing is called and the item says why.
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::with_env(AiEnv::none(), fake.clone());
    let (batch, item) = f.submitted(2);
    f.runner.run_batch(&batch.id).await.unwrap();
    assert!(fake.requests().is_empty());
    let failed = f.item(&item.id);
    assert_eq!(failed.status, IngestItemStatus::Failed);
    assert!(
        failed
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("AI is not configured")),
        "{:?}",
        failed.error
    );
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);

    // A key, but a client that refuses as not configured: same message on
    // the item, not "none of the photos could be described".
    let f = Fixture::new(AiState::disabled().client);
    let (batch, item) = f.submitted(2);
    f.runner.run_batch(&batch.id).await.unwrap();
    let refused = f.item(&item.id);
    assert_eq!(refused.status, IngestItemStatus::Failed);
    assert_eq!(refused.error, failed.error);
    assert!(
        f.photos(&item.id)
            .iter()
            .all(|p| p.status == IngestPhotoStatus::Failed && p.error == failed.error)
    );
}

/// Answers every call only once the test opens the gate, counting calls in
/// flight.
struct GatedClient {
    in_flight: AtomicUsize,
    peak: AtomicUsize,
    gate: Semaphore,
}

#[async_trait]
impl AiClient for GatedClient {
    async fn chat(&self, _: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        self.gate
            .acquire()
            .await
            .expect("gate never closes")
            .forget();
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let content = if image_urls(&request).is_empty() {
            suggestion("Mouse")
        } else {
            description("photo", "A mouse")
        };
        Ok(ChatResponse {
            content,
            usage: None,
            model: request.model,
        })
    }
}

/// Polls `condition` until it holds, failing the test after five seconds.
async fn wait_for(what: &str, condition: impl Fn() -> bool) {
    time::timeout(Duration::from_secs(5), async {
        while !condition() {
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallelism_is_bounded_by_the_semaphore() {
    let client = Arc::new(GatedClient {
        in_flight: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        gate: Semaphore::new(0),
    });
    let f = Fixture::new(client.clone());
    let (batch, item) = f.submitted(6);

    let runner = Arc::clone(&f.runner);
    let batch_id = batch.id.clone();
    let run = tokio::spawn(async move { runner.run_batch(&batch_id).await });
    wait_for("four calls in flight", || {
        client.in_flight.load(Ordering::SeqCst) >= 4
    })
    .await;
    // Give the other two photos every chance to (wrongly) get through.
    time::sleep(Duration::from_millis(200)).await;
    assert_eq!(client.in_flight.load(Ordering::SeqCst), 4);

    // Six vision calls and one synthesis.
    client.gate.add_permits(7);
    time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the run finishes")
        .unwrap()
        .unwrap();
    assert_eq!(client.peak.load(Ordering::SeqCst), 4);
    assert_eq!(f.item(&item.id).status, IngestItemStatus::Ready);
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

/// Panics on its first call and answers the rest from `fake`.
struct PanicsOnce {
    calls: AtomicUsize,
    fake: FakeAiClient,
}

#[async_trait]
impl AiClient for PanicsOnce {
    async fn chat(&self, config: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        assert!(
            self.calls.fetch_add(1, Ordering::SeqCst) > 0,
            "the scripted panic"
        );
        self.fake.chat(config, request).await
    }
}

#[tokio::test]
async fn a_panicking_call_fails_only_its_photo() {
    let client = Arc::new(PanicsOnce {
        calls: AtomicUsize::new(0),
        fake: FakeAiClient::new(),
    });
    client.fake.push(Ok(description("photo", "A mouse")));
    client.fake.push(Ok(suggestion("Mouse")));
    let f = Fixture::new(client.clone());
    let (batch, item) = f.submitted(2);

    f.runner.run_batch(&batch.id).await.unwrap();

    assert_eq!(
        errors_of(&f.photos(&item.id)),
        [None, Some("analysis task failed")]
    );
    assert_eq!(f.item(&item.id).status, IngestItemStatus::Ready);
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

#[tokio::test]
async fn requeue_on_startup_finishes_an_interrupted_batch() {
    // Review Focus 3: a restart left the item analysing mid-run.
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(1);
    f.set_item_status(&item.id, IngestItemStatus::Analysing);
    fake.push(Ok(description("photo", "A mouse")));
    fake.push(Ok(suggestion("Mouse")));

    let resumed = ingest::requeue_interrupted(&mut f.db.pool.get().unwrap()).unwrap();
    assert_eq!(resumed, [batch.id.as_str()]);
    assert_eq!(f.item(&item.id).status, IngestItemStatus::Queued);
    f.runner.run_batch(&batch.id).await.unwrap();

    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Ready);
    assert_eq!(suggestion_of(&item)["name"], "Mouse");
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

#[tokio::test]
async fn resume_interrupted_runs_processing_batches_in_the_background() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(1);
    f.set_item_status(&item.id, IngestItemStatus::Analysing);
    fake.push(Ok(description("photo", "A mouse")));
    fake.push(Ok(suggestion("Mouse")));

    assert_eq!(f.runner.resume_interrupted().await.unwrap(), 1);
    wait_for("the resumed batch to settle", || {
        f.batch(&batch.id).status == IngestBatchStatus::Reviewing
    })
    .await;
    assert_eq!(f.item(&item.id).status, IngestItemStatus::Ready);
}
