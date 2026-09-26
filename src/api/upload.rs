//! `POST /api/upload/{entity_id}`: a multipart photo upload.
//!
//! The form carries one `file` field and an optional `primary` flag. The file
//! is streamed to a temp file under `originals/` as it arrives, then handed to
//! [`upload::store`], which decides its type from its bytes (the client's
//! content type and filename are advisory) and files it under the primary
//! rule. The body is capped at [`MAX_UPLOAD_BYTES`] on this route only, and a
//! caller who may not write is refused before the body is read.
//!
//! The limit layer runs before the handler, so a request whose
//! `Content-Length` is over the limit gets 413 even from a caller who may not
//! write (who would otherwise get 403): the size is judged before the actor.
//!
//! Every error answers `{ "error": "<message>" }` with its status: 400 for a
//! missing or duplicate `file` field or a malformed form, 403, 404 for an
//! unknown entity, 413, 415 for a format we do not accept, 422 for an image
//! whose header does not decode, 500 (logged, with a path-free message).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::body::Bytes;
use axum::extract::multipart::{Field, MultipartError, MultipartRejection};
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router, middleware};
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::task;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::{error, info};

use crate::db::SqlitePool;
use crate::graphql::context::Actor;
use crate::kinds::AttachmentKind;
use crate::models::Attachment;
use crate::svc::attachment;
use crate::svc::upload::{self, Staged, TempUpload, UploadError};

/// The largest request body the upload route accepts: 25 MiB.
pub const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// Chunks buffered between the request stream and the temp-file writer.
const CHUNKS_IN_FLIGHT: usize = 8;

#[derive(Clone)]
struct UploadState {
    pool: SqlitePool,
    data_dir: Arc<PathBuf>,
}

/// The upload route, storing originals under `data_dir`.
pub fn upload_routes(pool: SqlitePool, data_dir: Arc<PathBuf>) -> Router {
    Router::new()
        .route("/api/upload/{entity_id}", post(upload))
        // The limit layer governs: axum's 2 MiB default for `Multipart`
        // would otherwise cut in first.
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(MAX_UPLOAD_BYTES))
        // Outside the limit layer, which refuses an over-long `Content-Length`
        // with a plain-text 413 before the handler runs.
        .layer(middleware::map_response(json_too_large))
        .with_state(UploadState { pool, data_dir })
}

/// Why an upload request failed; each variant maps to one status.
#[derive(Debug, thiserror::Error)]
enum UploadFailure {
    #[error("Forbidden: write access required")]
    Forbidden,
    #[error("the upload is larger than 25 MiB")]
    TooLarge,
    #[error("the form has no \"file\" field")]
    MissingFile,
    #[error("the form has more than one \"file\" field")]
    DuplicateFile,
    #[error("not a multipart form: {0}")]
    NotMultipart(#[from] MultipartRejection),
    #[error("malformed multipart body: {0}")]
    Malformed(MultipartError),
    #[error(transparent)]
    Store(#[from] UploadError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<MultipartError> for UploadFailure {
    fn from(err: MultipartError) -> Self {
        // A body that trips the limit mid-stream surfaces as a multipart error.
        if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
            Self::TooLarge
        } else {
            Self::Malformed(err)
        }
    }
}

impl UploadFailure {
    fn status(&self) -> StatusCode {
        match self {
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::MissingFile
            | Self::DuplicateFile
            | Self::NotMultipart(_)
            | Self::Malformed(_) => StatusCode::BAD_REQUEST,
            Self::Store(UploadError::NotFound) => StatusCode::NOT_FOUND,
            Self::Store(UploadError::Unsupported) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::Store(UploadError::Unreadable) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Store(UploadError::Io(_)) | Self::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl IntoResponse for UploadFailure {
    fn into_response(self) -> Response {
        let status = self.status();
        let message = if status.is_server_error() {
            // The chain may name paths: it goes to the log, not the client.
            error!("upload failed: {self:#}");
            "the upload could not be stored".to_owned()
        } else {
            self.to_string()
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Gives the limit layer's 413 the same JSON body as every other error.
async fn json_too_large(response: Response) -> Response {
    if response.status() == StatusCode::PAYLOAD_TOO_LARGE {
        UploadFailure::TooLarge.into_response()
    } else {
        response
    }
}

/// The stored attachment, shaped like the GraphQL `Attachment` at its
/// default thumbnail size.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadedAttachment {
    id: String,
    kind: AttachmentKind,
    primary: bool,
    title: String,
    mime_type: String,
    size_bytes: i64,
    url: String,
    thumbnail_url: Option<String>,
}

impl From<Attachment> for UploadedAttachment {
    fn from(att: Attachment) -> Self {
        let (url, thumbnail_url) = attachment::attachment_urls(&att);
        Self {
            id: att.id,
            kind: att.kind,
            primary: att.is_primary,
            title: att.title,
            mime_type: att.mime_type,
            size_bytes: att.size_bytes,
            url,
            thumbnail_url,
        }
    }
}

async fn upload(
    State(UploadState { pool, data_dir }): State<UploadState>,
    Extension(actor): Extension<Actor>,
    Path(entity_id): Path<String>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<UploadedAttachment>), UploadFailure> {
    // Before the body is touched, so a refused caller cannot make us buffer
    // or write anything.
    if !actor.can_write() {
        return Err(UploadFailure::Forbidden);
    }
    let form = receive(multipart?, attachment::originals_dir(&data_dir)).await?;
    let stored = task::spawn_blocking(move || {
        let mut conn = pool.get().context("getting a database connection")?;
        upload::store(
            &mut conn,
            &data_dir,
            &entity_id,
            form.staged,
            form.filename.as_deref(),
            form.primary,
        )
    })
    .await
    .context("the upload task failed")??;
    let att = stored.attachment;
    info!(attachment = %att.id, entity = %att.entity_id, "photo uploaded");
    Ok((StatusCode::CREATED, Json(att.into())))
}

/// What the form carried.
struct Form {
    staged: Staged,
    /// The client's name for the file; advisory, used only for the title.
    filename: Option<String>,
    primary: bool,
}

/// Reads the form, staging its one `file` field under `originals_dir`.
/// Fields other than `file` and `primary` are skipped.
async fn receive(mut multipart: Multipart, originals_dir: PathBuf) -> Result<Form, UploadFailure> {
    let mut file = None;
    let mut primary = false;
    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("file") if file.is_some() => return Err(UploadFailure::DuplicateFile),
            Some("file") => {
                let filename = field.file_name().map(str::to_owned);
                file = Some((stage(field, originals_dir.clone()).await?, filename));
            }
            Some("primary") => primary = parse_flag(&field.text().await?),
            _ => {}
        }
    }
    let (staged, filename) = file.ok_or(UploadFailure::MissingFile)?;
    Ok(Form {
        staged,
        filename,
        primary,
    })
}

/// Streams `field` into a new temp upload under `originals_dir`.
///
/// The file work runs on the blocking pool, fed chunk by chunk. A stream
/// error (a client that goes away, or the body limit tripping) is forwarded
/// to the writer, which then drops the temp file (removing it) without
/// flushing or syncing it. The writer is always awaited, so the temp file is
/// gone before the response is.
///
/// Each upload in flight holds one blocking-pool thread for as long as the
/// client takes to send it: acceptable for a household LAN app without auth,
/// where concurrent uploads are few.
async fn stage(mut field: Field<'_>, originals_dir: PathBuf) -> Result<Staged, UploadFailure> {
    let (tx, mut rx) = mpsc::channel::<Result<Bytes, MultipartError>>(CHUNKS_IN_FLIGHT);
    let writer = task::spawn_blocking(move || {
        let mut temp = TempUpload::create(&originals_dir)?;
        while let Some(chunk) = rx.blocking_recv() {
            temp.write(&chunk?)?;
        }
        Ok::<_, UploadFailure>(temp.finish()?)
    });
    while let Some(item) = field.chunk().await.transpose() {
        let failed = item.is_err();
        // A closed channel means the writer failed; awaiting it says why.
        if tx.send(item).await.is_err() || failed {
            break;
        }
    }
    drop(tx);
    writer.await.context("the upload writer failed")?
}

/// `primary` is read leniently: `true`, `1`, `on` (an HTML checkbox) and
/// `yes`, in any case, mean true; anything else means false.
fn parse_flag(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "on" | "yes"
    )
}

#[cfg(test)]
mod tests {
    use axum::body;

    use super::*;

    #[test]
    fn primary_flag_is_lenient() {
        for yes in ["true", "TRUE", " True ", "1", "on", "yes"] {
            assert!(parse_flag(yes), "{yes:?}");
        }
        for no in ["false", "0", "off", "", "maybe"] {
            assert!(!parse_flag(no), "{no:?}");
        }
    }

    #[test]
    fn store_errors_map_to_their_statuses() {
        for (err, status) in [
            (UploadError::NotFound, StatusCode::NOT_FOUND),
            (UploadError::Unsupported, StatusCode::UNSUPPORTED_MEDIA_TYPE),
            (UploadError::Unreadable, StatusCode::UNPROCESSABLE_ENTITY),
            (
                UploadError::Io(anyhow::anyhow!("disk full")),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ] {
            assert_eq!(UploadFailure::from(err).status(), status);
        }
    }

    #[tokio::test]
    async fn a_server_error_body_does_not_leak_its_cause() {
        let failure = UploadFailure::Store(UploadError::Io(
            anyhow::anyhow!("/srv/data/originals/.upload-x").context("writing"),
        ));

        let response = failure.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!text.contains("/srv"), "{text}");
        assert!(text.contains("\"error\""), "{text}");
    }
}
