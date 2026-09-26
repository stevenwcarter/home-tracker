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

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;
use tokio::task;
use tracing::info;

use crate::api::AppError;
use crate::api::blob::{self, BlobOwner};
use crate::api::upload::{self, UploadFailure};
use crate::db::SqlitePool;
use crate::graphql::context::Actor;
use crate::kinds::IngestPhotoStatus;
use crate::models::IngestPhoto;
use crate::svc::attachment::{self, DEFAULT_THUMB_URL_SIZE};
use crate::svc::blob::Blob;
use crate::svc::ingest;
use crate::svc::thumbnail_service::ThumbnailService;
use crate::svc::upload::store_ingest_photo;

#[derive(Clone)]
struct IngestState {
    pool: SqlitePool,
    data_dir: Arc<PathBuf>,
    thumbnails: Arc<ThumbnailService>,
}

/// The ingest routes: staging uploads into `data_dir`, and the staged
/// photos served from it and thumbnailed by `thumbnails`.
///
/// Only the upload writes, so only it reads `Extension<Actor>`.
pub fn ingest_routes(
    pool: SqlitePool,
    data_dir: Arc<PathBuf>,
    thumbnails: Arc<ThumbnailService>,
) -> Router {
    // The upload limits wrap the staging route alone, added before them.
    upload::with_upload_limits(
        Router::new().route("/api/ingest/items/{item_id}/photos", post(stage_photo)),
    )
    .route("/ingest/photos/{id}", get(original))
    .route("/ingest/photos/{id}/thumb/{size}", get(thumb))
    .with_state(IngestState {
        pool,
        data_dir,
        thumbnails,
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

/// Staged photo `id`, if it exists.
fn load(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<IngestPhoto>> {
    let mut conn = pool.get().context("db connection")?;
    ingest::get_photo(&mut conn, id)
}
