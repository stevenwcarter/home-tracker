//! `GET /attachments/{id}` (the original file) and
//! `GET /attachments/{id}/thumb/{size}` (a WebP thumbnail), served by
//! [`crate::api::blob`], which documents their caching and hardening headers.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::api::AppError;
use crate::api::blob::{self, BlobOwner};
use crate::db::SqlitePool;
use crate::models::Attachment;
use crate::svc::attachment;
use crate::svc::blob::Blob;
use crate::svc::thumbnail_service::ThumbnailService;

/// The attachment routes, with their state layered on. Originals are read
/// from the same data dir the thumbnail service reads, so the two can never
/// disagree.
///
/// Reads are open to every actor, so these handlers do not take
/// `Extension<Actor>`; one that needs to restrict access reads it from the
/// request as the GraphQL and upload handlers do (see `api::actor`).
pub fn attachment_routes(pool: SqlitePool, thumbnails: Arc<ThumbnailService>) -> Router {
    let data_dir = Arc::new(thumbnails.data_dir().to_path_buf());
    Router::new()
        .route("/attachments/{id}", get(original))
        .route("/attachments/{id}/thumb/{size}", get(thumb))
        .layer(Extension(pool))
        .layer(Extension(thumbnails))
        .layer(Extension(data_dir))
}

async fn original(
    Path(id): Path<String>,
    Extension(pool): Extension<SqlitePool>,
    Extension(data_dir): Extension<Arc<PathBuf>>,
) -> Result<Response, AppError> {
    let Some(att) = load(&pool, &id)? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    blob::serve_original(
        &data_dir,
        &Blob::from(&att),
        &att.title,
        BlobOwner::Attachment,
        &id,
    )
    .await
}

async fn thumb(
    Path((id, size)): Path<(String, String)>,
    Extension(pool): Extension<SqlitePool>,
    Extension(thumbnails): Extension<Arc<ThumbnailService>>,
) -> Result<Response, AppError> {
    let size = match blob::thumb_size(&size) {
        Ok(size) => size,
        Err(bad) => return Ok(bad.into_response()),
    };
    let Some(att) = load(&pool, &id)? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    blob::serve_thumb(
        &thumbnails,
        &Blob::from(&att),
        size,
        BlobOwner::Attachment,
        &id,
    )
    .await
}

/// Attachment `id`, if it exists.
fn load(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<Attachment>> {
    let mut conn = pool.get().context("db connection")?;
    attachment::get(&mut conn, id)
}
