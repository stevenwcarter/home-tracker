//! The ingest runner end to end against scripted model clients: parallel
//! description, synthesis, failures that stay local to a photo or an item,
//! retry, the model-call bound, events, and resuming after a restart.

// Each test crate uses a different subset of the shared support.
#[allow(dead_code)]
mod support;

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use home_tracker::ai::AiState;
use home_tracker::ai::client::{
    AiClient, AiError, ChatRequest, ChatResponse, ContentPart, Detail, ResponseFormat, Role,
};
use home_tracker::ai::env::AiEnv;
use home_tracker::ai::fake::FakeAiClient;
use home_tracker::ai::prompts::{SYNTHESIS_SYSTEM, VISION_SYSTEM};
use home_tracker::db::TestDb;
use home_tracker::ingest::events::{IngestEvent, IngestEvents};
use home_tracker::ingest::runner::{DESCRIBE_MAX_TOKENS, IngestRunner, SYNTHESIS_MAX_TOKENS};
use home_tracker::kinds::{IngestBatchStatus, IngestItemStatus, IngestPhotoStatus};
use home_tracker::models::{IngestBatch, IngestItem, IngestPhoto};
use home_tracker::schema::{settings, thumbnails};
use home_tracker::svc::ai_settings::{AiConfig, EXTRA_INSTRUCTIONS_KEY};
use home_tracker::svc::fixtures::{self, jpeg, seed_sample};
use home_tracker::svc::thumbnail_service::ThumbnailService;
use home_tracker::svc::{attachment, ingest};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::logs::Logs;
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
        let (batch, mut items) = self.submitted_items(&[photos]);
        (batch, items.remove(0))
    }

    /// A submitted batch with an item per entry of `photos`, each with that
    /// many distinct JPEG photos, their originals on disk.
    fn submitted_items(&self, photos: &[u32]) -> (IngestBatch, Vec<IngestItem>) {
        let mut conn = self.db.pool.get().unwrap();
        let (batch, first) = fixtures::ingest_batch(&mut conn, None);
        let mut items = vec![first];
        while items.len() < photos.len() {
            items.push(ingest::add_item(&mut conn, &batch.id).unwrap());
        }
        let mut seed = 0;
        for (item, &count) in items.iter().zip(photos) {
            for _ in 0..count {
                let bytes = jpeg(40 + seed, 30);
                seed += 1;
                let sha256 = hex::encode(Sha256::digest(&bytes));
                let path = attachment::original_path(self.data.path(), &sha256);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
                fixtures::ingest_photo(&mut conn, &item.id, &sha256, "image/jpeg");
            }
        }
        let batch = ingest::submit(&mut conn, &batch.id).unwrap();
        (batch, items)
    }

    /// Runs `sql` (one or more statements) on a connection of the test's own.
    fn execute(&self, sql: &str) {
        self.db.pool.get().unwrap().batch_execute(sql).unwrap();
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

    /// What `retryIngestItem` does before handing the item to the runner.
    fn retry(&self, id: &str) {
        ingest::retry(&mut self.db.pool.get().unwrap(), id).unwrap();
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

/// The `detail` of every image part of `request`.
fn image_details(request: &ChatRequest) -> Vec<Detail> {
    request
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|part| match part {
            ContentPart::ImageUrl { detail, .. } => Some(*detail),
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
        assert_eq!(image_details(vision), [Detail::Auto]);
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
        // The item twice: analysing, then ready.
        HashMap::from([("photo", 2), ("item", 2), ("batch", 1)])
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
    let mut events = f.runner.events().subscribe(&batch.id);
    f.retry(&item.id);
    fake.push(Ok(description("receipt", "Amazon receipt for a mouse")));
    fake.push(Ok(suggestion("Second guess")));
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
    // The retry sent the batch back to processing; the run settles it again.
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
    assert_eq!(
        drain(&mut events),
        HashMap::from([("photo", 1), ("item", 2), ("batch", 1)])
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

/// Answers every call (a description for a photo, else a suggestion) once
/// the gate lets it, counting calls in flight.
struct GatedClient {
    calls: AtomicUsize,
    in_flight: AtomicUsize,
    peak: AtomicUsize,
    gate: Semaphore,
}

impl GatedClient {
    /// A client whose gate lets `permits` calls through before it closes.
    fn new(permits: usize) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            gate: Semaphore::new(permits),
        })
    }
}

#[async_trait]
impl AiClient for GatedClient {
    async fn chat(&self, _: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
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
    let client = GatedClient::new(0);
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

#[tokio::test]
async fn extra_instructions_reach_both_system_prompts() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    diesel::replace_into(settings::table)
        .values((
            settings::key.eq(EXTRA_INSTRUCTIONS_KEY),
            settings::value.eq("Prefer metric"),
        ))
        .execute(&mut f.db.pool.get().unwrap())
        .unwrap();
    let (batch, _) = f.submitted(1);
    fake.push(Ok(description("photo", "A ruler")));
    fake.push(Ok(suggestion("Ruler")));

    f.runner.run_batch(&batch.id).await.unwrap();

    let requests = fake.requests();
    assert_eq!(requests.len(), 2);
    for (request, base) in requests.iter().zip([VISION_SYSTEM, SYNTHESIS_SYSTEM]) {
        let system = system_text(request);
        assert!(system.starts_with(base), "{system}");
        assert!(
            system.ends_with("Additional instructions from the user:\nPrefer metric"),
            "{system}"
        );
    }
}

#[tokio::test]
async fn a_failed_retry_does_not_keep_the_stale_suggestion() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(1);
    fake.push(Ok(description("photo", "A mouse")));
    fake.push(Ok(suggestion("First guess")));
    f.runner.run_batch(&batch.id).await.unwrap();
    assert_eq!(suggestion_of(&f.item(&item.id))["name"], "First guess");

    f.retry(&item.id);
    fake.push(Ok("I would rather not say.".to_owned()));
    f.runner.run_item(&item.id).await.unwrap();

    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Failed);
    assert_eq!(item.error.as_deref(), Some(NOT_JSON));
    assert_eq!(item.suggestion, None);
}

#[tokio::test]
async fn a_run_on_an_item_that_is_not_queued_changes_nothing() {
    let fake = Arc::new(FakeAiClient::new());
    let f = Fixture::new(fake.clone());
    let (batch, item) = f.submitted(1);
    f.set_item_status(&item.id, IngestItemStatus::Accepted);
    let before = f.item(&item.id);
    let mut events = f.runner.events().subscribe(&batch.id);

    f.runner.run_item(&item.id).await.unwrap();

    assert!(fake.requests().is_empty());
    assert_eq!(f.item(&item.id), before);
    assert!(
        f.photos(&item.id)
            .iter()
            .all(|p| p.status == IngestPhotoStatus::Pending)
    );
    // Only the batch moved: its one item is closed, so it is done.
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Done);
    assert_eq!(drain(&mut events), HashMap::from([("batch", 1)]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_runs_analyse_each_item_once_and_publish_one_batch_change() {
    let client = GatedClient::new(Semaphore::MAX_PERMITS);
    let f = Fixture::new(client.clone());
    let (batch, items) = f.submitted_items(&[1, 1]);
    let mut events = f.runner.events().subscribe(&batch.id);

    // A retry's run of the first item races the batch's run of both.
    let (batch_run, item_run) = tokio::join!(
        f.runner.run_batch(&batch.id),
        f.runner.run_item(&items[0].id)
    );
    batch_run.unwrap();
    item_run.unwrap();

    // Two photos and two syntheses: the first item was claimed only once.
    assert_eq!(client.calls.load(Ordering::SeqCst), 4);
    for item in &items {
        assert_eq!(f.item(&item.id).status, IngestItemStatus::Ready);
    }
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
    assert_eq!(
        drain(&mut events),
        HashMap::from([("photo", 2), ("item", 4), ("batch", 1)])
    );
}

/// Answers synthesis at once and vision calls in arrival order: the first
/// with `first`, every later one only once the gate opens.
struct HoldsLaterPhotos {
    vision_calls: AtomicUsize,
    held: AtomicUsize,
    gate: Semaphore,
    first: Result<String, AiError>,
}

impl HoldsLaterPhotos {
    fn new(first: Result<String, AiError>) -> Arc<Self> {
        Arc::new(Self {
            vision_calls: AtomicUsize::new(0),
            held: AtomicUsize::new(0),
            gate: Semaphore::new(0),
            first,
        })
    }
}

#[async_trait]
impl AiClient for HoldsLaterPhotos {
    async fn chat(&self, _: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        let content = if image_urls(&request).is_empty() {
            suggestion("Mouse")
        } else if self.vision_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.first.clone()?
        } else {
            self.held.fetch_add(1, Ordering::SeqCst);
            self.gate
                .acquire()
                .await
                .expect("gate never closes")
                .forget();
            description("photo", "A mouse")
        };
        Ok(ChatResponse {
            content,
            usage: None,
            model: request.model,
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_photo_that_cannot_be_stored_does_not_abort_its_sibling() {
    let client = HoldsLaterPhotos::new(Err(AiError::Transport("reset".to_owned())));
    let f = Fixture::new(client.clone());
    let (batch, item) = f.submitted(2);
    // Every write to a photo fails until the fault row goes.
    f.execute(
        "CREATE TABLE test_fault (id INTEGER);
         INSERT INTO test_fault VALUES (1);
         CREATE TRIGGER test_photo_fault BEFORE UPDATE ON ingest_photos
         WHEN EXISTS (SELECT 1 FROM test_fault)
         BEGIN SELECT RAISE(ABORT, 'injected fault'); END;",
    );

    let runner = Arc::clone(&f.runner);
    let batch_id = batch.id.clone();
    let run = tokio::spawn(async move { runner.run_batch(&batch_id).await });
    wait_for("the second photo's call", || {
        client.held.load(Ordering::SeqCst) == 1
    })
    .await;
    // Time for the first photo to fail and its failure not to be stored.
    time::sleep(Duration::from_millis(200)).await;
    f.execute("DELETE FROM test_fault;");
    client.gate.add_permits(1);
    time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the run finishes")
        .unwrap()
        .unwrap();

    // The sibling ran to the end instead of being aborted mid-call.
    let statuses: Vec<IngestPhotoStatus> = f.photos(&item.id).iter().map(|p| p.status).collect();
    assert!(
        statuses.contains(&IngestPhotoStatus::Described),
        "{statuses:?}"
    );
    assert!(
        statuses.contains(&IngestPhotoStatus::Pending),
        "{statuses:?}"
    );
    let item = f.item(&item.id);
    assert_eq!(item.status, IngestItemStatus::Failed);
    assert_eq!(item.error.as_deref(), Some("analysis task failed"));
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_settle_does_not_abort_the_other_items() {
    let client = HoldsLaterPhotos::new(Ok(description("photo", "A mouse")));
    let f = Fixture::new(client.clone());
    let (batch, items) = f.submitted_items(&[1, 1]);
    // A status Diesel cannot read makes every settle fail until restored.
    f.execute(&format!(
        "UPDATE ingest_batches SET status = 'bogus' WHERE id = '{}';",
        batch.id
    ));

    let runner = Arc::clone(&f.runner);
    let batch_id = batch.id.clone();
    let run = tokio::spawn(async move { runner.run_batch(&batch_id).await });
    wait_for("one item ready and the other's call held", || {
        client.held.load(Ordering::SeqCst) == 1
            && items
                .iter()
                .any(|item| f.item(&item.id).status == IngestItemStatus::Ready)
    })
    .await;
    // Time for the ready item's settle to fail.
    time::sleep(Duration::from_millis(200)).await;
    f.execute(&format!(
        "UPDATE ingest_batches SET status = 'processing' WHERE id = '{}';",
        batch.id
    ));
    client.gate.add_permits(1);
    let result = time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the run finishes")
        .unwrap();

    // The failed settle is reported, but only after every item finished.
    assert!(result.is_err());
    for item in &items {
        assert_eq!(f.item(&item.id).status, IngestItemStatus::Ready);
    }
    assert_eq!(f.batch(&batch.id).status, IngestBatchStatus::Reviewing);
}

#[tokio::test]
async fn a_batch_deleted_mid_run_calls_nothing_more_and_logs_no_error() {
    let (logs, _guard) = Logs::capture();
    let client = GatedClient::new(0);
    let f = Fixture::new(client.clone());
    // One photo more than the model-call bound, so one waits for a permit.
    let (batch, _) = f.submitted(5);

    let thumbnail_count = || -> i64 {
        thumbnails::table
            .count()
            .get_result(&mut f.db.pool.get().unwrap())
            .unwrap()
    };
    let seeded = thumbnail_count();

    let runner = Arc::clone(&f.runner);
    let batch_id = batch.id.clone();
    let run = tokio::spawn(async move { runner.run_batch(&batch_id).await });
    // Every photo prepared, so deleting the originals cannot fail one.
    wait_for("four calls in flight and every photo prepared", || {
        client.in_flight.load(Ordering::SeqCst) == 4 && thumbnail_count() == seeded + 5
    })
    .await;
    ingest::delete_batch(&mut f.db.pool.get().unwrap(), f.data.path(), &batch.id).unwrap();
    client.gate.add_permits(5);
    let result = time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the run finishes")
        .unwrap();

    // The waiting photo found its row gone once a permit freed, and was not
    // sent to the model.
    assert_eq!(client.calls.load(Ordering::SeqCst), 4);
    assert!(result.is_err(), "the batch is gone");
    let text = logs.text();
    assert!(!text.contains("ERROR"), "{text}");
    assert!(text.contains("batch removed mid-run"), "{text}");
}
