//! The AI ingest HTTP routes:
//!
//! - `POST /api/ingest/items/{item_id}/photos` stages a photo for an ingest
//!   item. It is the photo upload of `api::upload` (same form, limits,
//!   sniffing, statuses and JSON errors) storing an ingest photo instead of
//!   an attachment, plus 404 for an unknown item and 409 once the item's
//!   batch is no longer collecting.
//! - `GET /ingest/photos/{id}` and `GET /ingest/photos/{id}/thumb/{size}`
//!   serve a staged photo exactly as the attachment routes serve an
//!   attachment, through [`crate::api::blob`].
//! - `GET /api/ingest/batches/{id}/events` streams the batch's progress as
//!   server-sent events named `photo`, `item` and `batch`, each with data
//!   `{"id": …}` naming the row that changed, and a `ping` comment every
//!   15 s. It ends once the batch is done or deleted.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;
use serde_json::json;
use tokio::task;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{self as stream, StreamExt};
use tracing::{info, warn};

use crate::api::AppError;
use crate::api::blob::{self, BlobOwner};
use crate::api::upload::{self, UploadFailure};
use crate::db::SqlitePool;
use crate::graphql::context::Actor;
use crate::ingest::events::{EventKind, IngestEvent, IngestEvents};
use crate::kinds::{IngestBatchStatus, IngestPhotoStatus};
use crate::models::IngestPhoto;
use crate::svc::attachment::{self, DEFAULT_THUMB_URL_SIZE};
use crate::svc::blob::Blob;
use crate::svc::ingest;
use crate::svc::thumbnail_service::ThumbnailService;
use crate::svc::upload::store_ingest_photo;

/// How often an idle progress stream sends its `ping` comment, so proxies
/// and the browser keep the connection open.
const KEEP_ALIVE_EVERY: Duration = Duration::from_secs(15);

#[derive(Clone)]
struct IngestState {
    pool: SqlitePool,
    data_dir: Arc<PathBuf>,
    thumbnails: Arc<ThumbnailService>,
    events: Arc<IngestEvents>,
}

/// The ingest routes: staging uploads into `data_dir`, the staged photos
/// served from it and thumbnailed by `thumbnails`, and the progress streams
/// fed by `events`.
///
/// Only the upload writes, so only it reads `Extension<Actor>`.
pub fn ingest_routes(
    pool: SqlitePool,
    data_dir: Arc<PathBuf>,
    thumbnails: Arc<ThumbnailService>,
    events: Arc<IngestEvents>,
) -> Router {
    // The upload limits wrap the staging route alone, added before them.
    upload::with_upload_limits(
        Router::new().route("/api/ingest/items/{item_id}/photos", post(stage_photo)),
    )
    .route("/ingest/photos/{id}", get(original))
    .route("/ingest/photos/{id}/thumb/{size}", get(thumb))
    .route("/api/ingest/batches/{id}/events", get(progress))
    .with_state(IngestState {
        pool,
        data_dir,
        thumbnails,
        events,
    })
}

/// The staged photo, shaped like the GraphQL `IngestPhoto` at its default
/// thumbnail size.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StagedPhoto {
    id: String,
    position: i32,
    status: IngestPhotoStatus,
    title: String,
    mime_type: String,
    size_bytes: i64,
    url: String,
    thumbnail_url: Option<String>,
}

impl From<IngestPhoto> for StagedPhoto {
    fn from(photo: IngestPhoto) -> Self {
        let url = ingest::original_url(&photo);
        let thumbnail_url = ingest::thumbnail_url(&photo, DEFAULT_THUMB_URL_SIZE);
        Self {
            id: photo.id,
            position: photo.position,
            status: photo.status,
            title: photo.title,
            mime_type: photo.mime_type,
            size_bytes: photo.size_bytes,
            url,
            thumbnail_url,
        }
    }
}

async fn stage_photo(
    State(IngestState { pool, data_dir, .. }): State<IngestState>,
    Extension(actor): Extension<Actor>,
    Path(item_id): Path<String>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<StagedPhoto>), UploadFailure> {
    // Before the body is touched, so a refused caller cannot make us buffer
    // or write anything.
    if !actor.can_write() {
        return Err(UploadFailure::Forbidden);
    }
    // A `primary` field means nothing here and is ignored.
    let form = upload::receive(multipart?, attachment::originals_dir(&data_dir)).await?;
    let photo = task::spawn_blocking(move || {
        let mut conn = pool.get().context("getting a database connection")?;
        store_ingest_photo(
            &mut conn,
            &data_dir,
            &item_id,
            form.staged,
            form.filename.as_deref(),
        )
    })
    .await
    .context("the upload task failed")??;
    info!(photo = %photo.id, item = %photo.item_id, "ingest photo staged");
    Ok((StatusCode::CREATED, Json(photo.into())))
}

async fn original(
    Path(id): Path<String>,
    State(IngestState { pool, data_dir, .. }): State<IngestState>,
) -> Result<Response, AppError> {
    let Some(photo) = load(&pool, &id)? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    blob::serve_original(
        &data_dir,
        &Blob::from(&photo),
        &photo.title,
        BlobOwner::IngestPhoto,
        &id,
    )
    .await
}

async fn thumb(
    Path((id, size)): Path<(String, String)>,
    State(IngestState {
        pool, thumbnails, ..
    }): State<IngestState>,
) -> Result<Response, AppError> {
    let size = match blob::thumb_size(&size) {
        Ok(size) => size,
        Err(bad) => return Ok(bad.into_response()),
    };
    let Some(photo) = load(&pool, &id)? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    blob::serve_thumb(
        &thumbnails,
        &Blob::from(&photo),
        size,
        BlobOwner::IngestPhoto,
        &id,
    )
    .await
}

/// Batch `id`'s progress stream. It subscribes before loading the batch, so
/// no change between the load and the subscription is missed; a client
/// refetches the batch when the stream opens, so the stream carries only
/// what changes after that. A batch that is already done gets one `batch`
/// event and the stream ends.
async fn progress(
    Path(id): Path<String>,
    State(IngestState { pool, events, .. }): State<IngestState>,
) -> Result<Response, AppError> {
    let receiver = events.subscribe(&id);
    let batch_id = id.clone();
    let loaded = task::spawn_blocking(move || {
        let mut conn = pool.get().context("db connection")?;
        ingest::get_batch(&mut conn, &batch_id)
    })
    .await
    .context("the batch load task failed")
    .and_then(|loaded| loaded);
    let batch = match loaded {
        Ok(Some(batch)) => batch,
        // Nobody else can be watching a batch that does not exist, and a
        // failed load answers with an error; either way, closing drops the
        // channel this request opened.
        missing_or_failed => {
            events.close(&id);
            return Ok(missing_or_failed.map(|_| StatusCode::NOT_FOUND.into_response())?);
        }
    };
    let done = batch.status == IngestBatchStatus::Done;
    if done {
        // The receiver sees the channel closed once it has read what was
        // already sent, which ends the live part of the stream.
        events.close(&id);
    }
    let snapshot = done.then(|| IngestEvent::new(EventKind::Batch, id.as_str()));
    let live = BroadcastStream::new(receiver).map(move |received| match received {
        Ok(event) => event,
        // The client refetches the whole batch on any event, so one
        // `batch` event stands in for everything it missed.
        Err(BroadcastStreamRecvError::Lagged(missed)) => {
            warn!(batch = %id, missed, "ingest progress stream lagged");
            IngestEvent::new(EventKind::Batch, id.as_str())
        }
    });
    let stream = stream::iter(snapshot).chain(live).map(sse_event);
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE_EVERY).text("ping"))
        .into_response())
}

/// `event` as a server-sent event named after its kind, data `{"id": …}`.
fn sse_event(event: IngestEvent) -> Result<Event, axum::Error> {
    Event::default()
        .event(event.kind.as_str())
        .json_data(json!({ "id": event.id }))
}

/// Staged photo `id`, if it exists.
fn load(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<IngestPhoto>> {
    let mut conn = pool.get().context("db connection")?;
    ingest::get_photo(&mut conn, id)
}
