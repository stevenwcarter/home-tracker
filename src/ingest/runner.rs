//! Runs submitted ingest batches: each item's photos are described by the
//! vision model in parallel, then the synthesis model combines the item's
//! descriptions into a suggestion (spec §5).
//!
//! Items of a batch run concurrently, and so do the photos of an item; one
//! process-wide semaphore bounds the model calls in flight, and a permit is
//! held only around the call itself. Database work runs on the blocking
//! pool with a connection taken there, so no connection is ever held across
//! an await. Every step runs in its own task: a panic fails that photo or
//! item ("analysis task failed") and the rest of the batch carries on.
//! Each photo and item result, and each change of the batch status, is
//! published on the batch's [`IngestEvents`] channel; an item starting
//! analysis is not, since the client learns of it from its photos' results
//! and its own.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::prelude::{BASE64_STANDARD, Engine as _};
use chrono::{TimeDelta, Utc};
use diesel::prelude::*;
use tokio::sync::Semaphore;
use tokio::task::{self, JoinSet};
use tokio::time::{self, Instant};
use tracing::{error, info, warn};

use crate::ai::AiState;
use crate::ai::client::{
    AiError, ChatRequest, ChatResponse, ContentPart, Detail, Message, ResponseFormat, Role,
};
use crate::ai::prompts::{self, SYNTHESIS_SYSTEM, VISION_SYSTEM};
use crate::db::SqlitePool;
use crate::ingest::events::{EventKind, IngestEvent, IngestEvents};
use crate::ingest::parse::{self, ParseError, PhotoDescription};
use crate::kinds::{IngestBatchStatus, IngestItemStatus, IngestPhotoStatus};
use crate::models::{IngestPhoto, Thumbnail};
use crate::svc::ai_settings::{self, AiConfig};
use crate::svc::blob::Blob;
use crate::svc::ingest::{self, IngestError, IngestRecord};
use crate::svc::thumbnail::ThumbSize;
use crate::svc::thumbnail_service::ThumbnailService;
use crate::svc::{settings, tag};

/// The output cap of a vision call. Reasoning tokens count against it, so
/// it leaves room for reasoning on top of the description itself.
pub const DESCRIBE_MAX_TOKENS: u32 = 6000;
/// The output cap of a synthesis call, reasoning included.
pub const SYNTHESIS_MAX_TOKENS: u32 = 6000;
/// How many model calls may be in flight across the whole process.
const MODEL_CALLS: usize = 4;
/// A batch with no activity for this long is abandoned and deleted.
pub const STALE_AFTER: TimeDelta = TimeDelta::days(7);
/// How often abandoned batches are looked for after startup.
pub const CLEANUP_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The error of a photo or item whose task panicked or hit a database error.
const TASK_FAILED: &str = "analysis task failed";
/// The error of an item none of whose photos could be described.
const NOTHING_DESCRIBED: &str = "None of this item's photos could be described";

/// Why one photo could not be described; `Display` is the photo's error,
/// which the user sees, so it names no path, hash or key.
#[derive(Debug, thiserror::Error)]
enum PhotoFailure {
    #[error("This photo could not be read as an image")]
    NotAnImage,
    #[error("This photo could not be prepared for the model")]
    NotPrepared,
    #[error(transparent)]
    Ai(#[from] AiError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

/// How a photo's task ended, as far as its item cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhotoOutcome {
    /// Described or failed; the item synthesises from whatever was described.
    Settled,
    /// The client refused for lack of a key: the item fails as a whole.
    NotConfigured,
}

/// Describes and synthesises submitted batches in the background.
pub struct IngestRunner {
    pool: SqlitePool,
    thumbnails: Arc<ThumbnailService>,
    ai: Arc<AiState>,
    events: Arc<IngestEvents>,
    permits: Semaphore,
}

impl IngestRunner {
    /// A runner reading originals through `thumbnails` (whose data dir is
    /// also where cleanup removes files) and calling the model through `ai`.
    pub fn new(
        pool: SqlitePool,
        thumbnails: Arc<ThumbnailService>,
        ai: Arc<AiState>,
        events: Arc<IngestEvents>,
    ) -> Arc<Self> {
        Arc::new(Self {
            pool,
            thumbnails,
            ai,
            events,
            permits: Semaphore::new(MODEL_CALLS),
        })
    }

    /// The change events this runner publishes, for the progress stream.
    pub fn events(&self) -> &Arc<IngestEvents> {
        &self.events
    }

    /// Runs batch `batch_id` in the background; returns immediately.
    pub fn spawn_batch(self: &Arc<Self>, batch_id: String) {
        let runner = Arc::clone(self);
        tokio::spawn(async move {
            log_failure("batch", &batch_id, runner.run_batch(&batch_id).await);
        });
    }

    /// Runs item `item_id` (a retry) in the background; returns immediately.
    pub fn spawn_item(self: &Arc<Self>, item_id: String) {
        let runner = Arc::clone(self);
        tokio::spawn(async move {
            log_failure("item", &item_id, runner.run_item(&item_id).await);
        });
    }

    /// Analyses every queued item of batch `batch_id`, then settles it.
    pub async fn run_batch(self: &Arc<Self>, batch_id: &str) -> Result<()> {
        let id = batch_id.to_owned();
        let queued = self
            .db(move |conn| {
                Ok(ingest::items(conn, &id)?
                    .into_iter()
                    .filter(|item| item.status == IngestItemStatus::Queued)
                    .map(|item| item.id)
                    .collect())
            })
            .await?;
        self.run_items(batch_id, queued).await
    }

    /// Analyses item `item_id` again (its photos not yet described, then the
    /// synthesis), then settles its batch.
    pub async fn run_item(self: &Arc<Self>, item_id: &str) -> Result<()> {
        let id = item_id.to_owned();
        let item = self
            .db(move |conn| {
                ingest::get_item(conn, &id)?
                    .ok_or_else(|| IngestError::NotFound(IngestRecord::Item).into())
            })
            .await?;
        self.run_items(&item.batch_id, vec![item.id]).await
    }

    /// After a restart: puts interrupted items back in the queue and resumes
    /// every processing batch in the background. Returns how many resumed.
    pub async fn resume_interrupted(self: &Arc<Self>) -> Result<usize> {
        let batches = self.db(ingest::requeue_interrupted).await?;
        let count = batches.len();
        for batch_id in batches {
            self.spawn_batch(batch_id);
        }
        Ok(count)
    }

    /// Deletes every batch idle for [`STALE_AFTER`] and logs how many went.
    pub async fn cleanup_stale(&self) -> Result<usize> {
        let data_dir = self.thumbnails.data_dir().to_path_buf();
        let cutoff = Utc::now().naive_utc() - STALE_AFTER;
        let removed = self
            .db(move |conn| ingest::cleanup_stale(conn, &data_dir, cutoff))
            .await?;
        info!(removed, "removed abandoned ingest batches");
        Ok(removed)
    }

    /// Runs [`Self::cleanup_stale`] every [`CLEANUP_EVERY`], starting one
    /// period from now (startup runs its own cleanup).
    pub fn spawn_cleanup_schedule(self: &Arc<Self>) {
        let runner = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticks = time::interval_at(Instant::now() + CLEANUP_EVERY, CLEANUP_EVERY);
            loop {
                ticks.tick().await;
                if let Err(err) = runner.cleanup_stale().await {
                    warn!("ingest cleanup failed: {err:#}");
                }
            }
        });
    }

    /// Analyses `item_ids` of batch `batch_id` concurrently, settling the
    /// batch as each finishes and once more at the end. An item whose task
    /// fails is marked failed. A failing step never cuts the other items
    /// short (dropping the set would abort them mid-analysis): it is logged,
    /// and the first such error is returned once every item is done.
    async fn run_items(self: &Arc<Self>, batch_id: &str, item_ids: Vec<String>) -> Result<()> {
        let mut tasks = Tasks::default();
        for item_id in item_ids {
            let runner = Arc::clone(self);
            let id = item_id.clone();
            tasks.spawn(item_id, async move { runner.analyse_item(&id).await });
        }
        let mut first_error = None;
        while let Some((item_id, result)) = tasks.next().await {
            if let Err(reason) = result {
                error!(item = %item_id, "ingest item analysis failed: {reason}");
                if let Err(err) = self.fail_item(batch_id, &item_id, TASK_FAILED).await {
                    error!(item = %item_id, "could not mark the ingest item failed: {err:#}");
                }
            }
            if let Err(err) = self.settle(batch_id).await {
                error!(batch = %batch_id, "could not settle the ingest batch: {err:#}");
                first_error.get_or_insert(err);
            }
        }
        // Covers a run with no items and a settle that failed above.
        self.settle(batch_id).await?;
        first_error.map_or(Ok(()), Err)
    }

    /// Claims queued item `item_id` (any other status: nothing to do),
    /// describes its photos that are not described yet, all at once, then
    /// synthesises its suggestion from every described photo.
    async fn analyse_item(self: Arc<Self>, item_id: &str) -> Result<()> {
        let id = item_id.to_owned();
        let env = self.ai.env.clone();
        let claimed = self
            .db(move |conn| {
                let Some(item) = ingest::claim_for_analysis(conn, &id)? else {
                    return Ok(None);
                };
                let photos = ingest::photos(conn, &id)?;
                Ok(Some((
                    item.batch_id,
                    photos,
                    ai_settings::config(conn, &env)?,
                )))
            })
            .await?;
        let Some((batch_id, photos, config)) = claimed else {
            return Ok(());
        };
        self.publish(&batch_id, EventKind::Item, item_id);
        let Some(config) = config else {
            return self.fail_not_configured(&batch_id, item_id).await;
        };
        let config = Arc::new(config);

        let total = photos.len();
        let mut tasks = Tasks::default();
        for (index, photo) in photos.into_iter().enumerate() {
            if photo.status == IngestPhotoStatus::Described {
                continue;
            }
            let runner = Arc::clone(&self);
            let config = Arc::clone(&config);
            let batch = batch_id.clone();
            tasks.spawn(photo.id.clone(), async move {
                runner
                    .describe_photo(&config, &batch, photo, index + 1, total)
                    .await
            });
        }
        // Every photo runs to the end whatever happens to another: the first
        // error is returned only once the set is drained.
        let mut not_configured = false;
        let mut first_error = None;
        while let Some((photo_id, result)) = tasks.next().await {
            match result {
                Ok(outcome) => not_configured |= outcome == PhotoOutcome::NotConfigured,
                Err(reason) => {
                    error!(photo = %photo_id, "describing an ingest photo failed: {reason}");
                    if let Err(err) = self.fail_photo(&batch_id, &photo_id, TASK_FAILED).await {
                        error!(photo = %photo_id, "could not mark the ingest photo failed: {err:#}");
                        first_error.get_or_insert(err);
                    }
                }
            }
        }
        if let Some(err) = first_error {
            return Err(err);
        }
        if not_configured {
            return self.fail_not_configured(&batch_id, item_id).await;
        }
        self.synthesise_item(&config, &batch_id, item_id).await
    }

    /// Describes `photo` (1-based `position` of its item's `total`), stores
    /// the description or the failure, and publishes the change.
    async fn describe_photo(
        &self,
        config: &AiConfig,
        batch_id: &str,
        photo: IngestPhoto,
        position: usize,
        total: usize,
    ) -> Result<PhotoOutcome> {
        match self.describe(config, &photo, position, total).await {
            Ok(description) => {
                let id = photo.id.clone();
                let json = description.to_json();
                self.db(move |conn| {
                    ingest::mark_photo_described(conn, &id, &json, description.kind)
                })
                .await?;
                self.publish(batch_id, EventKind::Photo, &photo.id);
                Ok(PhotoOutcome::Settled)
            }
            Err(failure) => {
                warn!(photo = %photo.id, "ingest photo not described: {failure}");
                self.fail_photo(batch_id, &photo.id, &failure.to_string())
                    .await?;
                Ok(match failure {
                    PhotoFailure::Ai(AiError::NotConfigured) => PhotoOutcome::NotConfigured,
                    _ => PhotoOutcome::Settled,
                })
            }
        }
    }

    /// One vision call: the photo's largest thumbnail as a data URL.
    async fn describe(
        &self,
        config: &AiConfig,
        photo: &IngestPhoto,
        position: usize,
        total: usize,
    ) -> Result<PhotoDescription, PhotoFailure> {
        let thumbnail = match self
            .thumbnails
            .get_or_generate(&Blob::from(photo), ThumbSize::LARGEST)
            .await
        {
            Ok(Some(thumbnail)) => thumbnail,
            Ok(None) => return Err(PhotoFailure::NotAnImage),
            Err(err) => {
                error!(photo = %photo.id, "could not prepare an ingest photo: {err:#}");
                return Err(PhotoFailure::NotPrepared);
            }
        };
        let request = ChatRequest {
            model: config.vision_model.clone(),
            messages: vec![
                system_message(VISION_SYSTEM, config),
                Message {
                    role: Role::User,
                    content: vec![
                        ContentPart::Text(prompts::describe_user_message(position, total)),
                        ContentPart::ImageUrl {
                            url: data_url(&thumbnail),
                            detail: Detail::Auto,
                        },
                    ],
                },
            ],
            response_format: Some(ResponseFormat::JsonSchema {
                name: "photo_description".to_owned(),
                schema: prompts::photo_description_schema(),
            }),
            max_tokens: Some(DESCRIBE_MAX_TOKENS),
        };
        let answer = self.call(config, request).await?;
        Ok(parse::parse_description(&answer.content)?)
    }

    /// One synthesis call over every described photo of item `item_id`; the
    /// item ends ready with the suggestion, or failed with a readable error.
    async fn synthesise_item(
        &self,
        config: &AiConfig,
        batch_id: &str,
        item_id: &str,
    ) -> Result<()> {
        let id = item_id.to_owned();
        let (described, currency, tag_names) = self
            .db(move |conn| {
                let described = described_photos(&ingest::photos(conn, &id)?);
                let tag_names: Vec<String> =
                    tag::list(conn)?.into_iter().map(|tag| tag.name).collect();
                Ok((described, settings::currency(conn)?, tag_names))
            })
            .await?;
        if described.is_empty() {
            return self.fail_item(batch_id, item_id, NOTHING_DESCRIBED).await;
        }
        let described: Vec<(usize, &PhotoDescription)> = described
            .iter()
            .map(|(position, description)| (*position, description))
            .collect();
        let request = ChatRequest {
            model: config.synthesis_model.clone(),
            messages: vec![
                system_message(SYNTHESIS_SYSTEM, config),
                Message {
                    role: Role::User,
                    content: vec![ContentPart::Text(prompts::synthesis_user_message(
                        &described, &currency, &tag_names,
                    ))],
                },
            ],
            response_format: Some(ResponseFormat::JsonSchema {
                name: "item_suggestion".to_owned(),
                schema: prompts::item_suggestion_schema(),
            }),
            max_tokens: Some(SYNTHESIS_MAX_TOKENS),
        };
        let suggestion = match self.call(config, request).await {
            Ok(answer) => {
                parse::parse_suggestion(&answer.content, &tag_names).map_err(|e| e.to_string())
            }
            Err(err) => Err(err.to_string()),
        };
        let suggestion = match suggestion {
            Ok(suggestion) => {
                serde_json::to_value(&suggestion).context("encoding an item suggestion")?
            }
            Err(message) => {
                warn!(item = %item_id, "ingest item not synthesised: {message}");
                return self.fail_item(batch_id, item_id, &message).await;
            }
        };
        let id = item_id.to_owned();
        self.db(move |conn| {
            conn.transaction(|conn| {
                ingest::set_item_suggestion(conn, &id, Some(&suggestion))?;
                ingest::set_item_status(conn, &id, IngestItemStatus::Ready, None)
            })
        })
        .await?;
        self.publish(batch_id, EventKind::Item, item_id);
        Ok(())
    }

    /// One model call, holding a permit for exactly its duration.
    async fn call(&self, config: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        // The semaphore is never closed; the error is unreachable in practice.
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| AiError::Transport("the model-call limiter is closed".to_owned()))?;
        self.ai.client.chat(config, request).await
    }

    /// Moves batch `batch_id` on if its items allow, publishing a change and
    /// closing the batch's streams once it is done.
    async fn settle(&self, batch_id: &str) -> Result<()> {
        let id = batch_id.to_owned();
        // One transaction, so two items settling at once cannot both see
        // (and publish) the same change.
        let (before, after) = self
            .db(move |conn| {
                conn.immediate_transaction(|conn| {
                    let before = ingest::get_batch(conn, &id)?.map(|batch| batch.status);
                    Ok((before, ingest::settle_batch(conn, &id)?.status))
                })
            })
            .await?;
        if before != Some(after) {
            self.publish(batch_id, EventKind::Batch, batch_id);
        }
        if after == IngestBatchStatus::Done {
            self.events.close(batch_id);
        }
        Ok(())
    }

    /// Marks photo `photo_id` failed with `message` and publishes it.
    async fn fail_photo(&self, batch_id: &str, photo_id: &str, message: &str) -> Result<()> {
        let (id, message) = (photo_id.to_owned(), message.to_owned());
        self.db(move |conn| ingest::mark_photo_failed(conn, &id, &message))
            .await?;
        self.publish(batch_id, EventKind::Photo, photo_id);
        Ok(())
    }

    /// Marks item `item_id` failed with `message` and publishes it.
    async fn fail_item(&self, batch_id: &str, item_id: &str, message: &str) -> Result<()> {
        let (id, message) = (item_id.to_owned(), message.to_owned());
        self.db(move |conn| {
            ingest::set_item_status(conn, &id, IngestItemStatus::Failed, Some(&message))
        })
        .await?;
        self.publish(batch_id, EventKind::Item, item_id);
        Ok(())
    }

    /// Fails item `item_id` because there is no API key to call with.
    async fn fail_not_configured(&self, batch_id: &str, item_id: &str) -> Result<()> {
        self.fail_item(batch_id, item_id, &AiError::NotConfigured.to_string())
            .await
    }

    fn publish(&self, batch_id: &str, kind: EventKind, id: &str) {
        self.events.publish(batch_id, IngestEvent::new(kind, id));
    }

    /// Runs `work` on the blocking pool with a connection taken there.
    async fn db<T, F>(&self, work: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut SqliteConnection) -> Result<T> + Send + 'static,
    {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let mut conn = pool.get().context("could not get a database connection")?;
            work(&mut conn)
        })
        .await
        .context("a database task panicked")?
    }
}

/// Concurrent tasks, each known by the id of the row it works on, so a
/// failure (an error or a panic) can be pinned on that row.
struct Tasks<T> {
    set: JoinSet<Result<T>>,
    rows: HashMap<task::Id, String>,
}

impl<T> Default for Tasks<T> {
    fn default() -> Self {
        Self {
            set: JoinSet::new(),
            rows: HashMap::new(),
        }
    }
}

impl<T: Send + 'static> Tasks<T> {
    fn spawn(&mut self, row: String, work: impl Future<Output = Result<T>> + Send + 'static) {
        let handle = self.set.spawn(work);
        self.rows.insert(handle.id(), row);
    }

    /// The next task to finish: its row and its value, or why it failed.
    async fn next(&mut self) -> Option<(String, Result<T, String>)> {
        let (task, result) = match self.set.join_next_with_id().await? {
            Ok((task, Ok(value))) => (task, Ok(value)),
            Ok((task, Err(err))) => (task, Err(format!("{err:#}"))),
            Err(err) => (err.id(), Err(err.to_string())),
        };
        Some((self.rows.remove(&task).unwrap_or_default(), result))
    }
}

/// The described photos of `photos` with their 1-based positions in the
/// list (the numbering the vision step used).
fn described_photos(photos: &[IngestPhoto]) -> Vec<(usize, PhotoDescription)> {
    photos
        .iter()
        .enumerate()
        .filter(|(_, photo)| photo.status == IngestPhotoStatus::Described)
        .filter_map(|(index, photo)| {
            let stored = photo.description.as_deref()?;
            match parse::parse_description(stored) {
                Ok(description) => Some((index + 1, description)),
                Err(err) => {
                    warn!(photo = %photo.id, "stored ingest description unreadable: {err}");
                    None
                }
            }
        })
        .collect()
}

/// The system message: `base` with the user's extra instructions.
fn system_message(base: &str, config: &AiConfig) -> Message {
    Message {
        role: Role::System,
        content: vec![ContentPart::Text(prompts::with_extra_instructions(
            base,
            config.extra_instructions.as_deref(),
        ))],
    }
}

/// `thumbnail` as a base64 `data:` URL.
fn data_url(thumbnail: &Thumbnail) -> String {
    format!(
        "data:{};base64,{}",
        thumbnail.mime_type,
        BASE64_STANDARD.encode(&thumbnail.data)
    )
}

/// Logs a background run's failure; there is no caller to return it to.
fn log_failure(what: &str, id: &str, result: Result<()>) {
    if let Err(err) = result {
        error!(%id, "ingest {what} run failed: {err:#}");
    }
}
