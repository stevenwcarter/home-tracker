//! `GET /attachments/{id}` (the original file) and
//! `GET /attachments/{id}/thumb/{size}` (a WebP thumbnail).
//!
//! Both are immutably cacheable: the SPA's URLs carry a version (`?v=`), and
//! an original's bytes are addressed by their sha256, which is also the ETag.

use std::fmt::Write as _;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::body::Body;
use axum::extract::{Extension, Path};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::api::{AppError, IMMUTABLE_CACHE};
use crate::db::SqlitePool;
use crate::svc::attachment;
use crate::svc::thumbnail::allowed_size;
use crate::svc::thumbnail_service::ThumbnailService;

/// The attachment routes, with their state layered on. Originals are read
/// from the same data dir the thumbnail service reads, so the two can never
/// disagree.
pub fn attachment_routes(pool: SqlitePool, thumbnails: Arc<ThumbnailService>) -> Router {
    let data_dir = Arc::new(thumbnails.data_dir().to_path_buf());
    Router::new()
        .route("/attachments/{id}", get(original))
        .route("/attachments/{id}/thumb/{size}", get(thumb))
        .layer(Extension(pool))
        .layer(Extension(thumbnails))
        .layer(Extension(data_dir))
}

const OCTET_STREAM: HeaderValue = HeaderValue::from_static("application/octet-stream");
const INLINE: HeaderValue = HeaderValue::from_static("inline");

async fn original(
    Path(id): Path<String>,
    Extension(pool): Extension<SqlitePool>,
    Extension(data_dir): Extension<Arc<PathBuf>>,
) -> Result<Response, AppError> {
    let att = {
        let mut conn = pool.get().context("db connection")?;
        attachment::get(&mut conn, &id)?
    };
    let Some(att) = att else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let path = attachment::original_path(&data_dir, &att.sha256);
    let file = match File::open(&path).await {
        Err(err) if err.kind() == ErrorKind::NotFound => {
            tracing::warn!(%id, path = %path.display(), "original file missing");
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
        opened => opened.with_context(|| format!("opening {}", path.display()))?,
    };
    // The file's own length, not `size_bytes`: a stale row must not make the
    // response lie about its body.
    let len = file
        .metadata()
        .await
        .with_context(|| format!("stat {}", path.display()))?
        .len();
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&att.mime_type).unwrap_or(OCTET_STREAM),
            ),
            (header::CONTENT_LENGTH, HeaderValue::from(len)),
            (header::CONTENT_DISPOSITION, content_disposition(&att.title)),
            (header::CACHE_CONTROL, IMMUTABLE_CACHE),
            (header::ETAG, etag(&att.sha256)?),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response())
}

async fn thumb(
    Path((id, size)): Path<(String, String)>,
    Extension(thumbnails): Extension<Arc<ThumbnailService>>,
) -> Result<Response, AppError> {
    let Some(size) = size.parse::<i64>().ok().and_then(allowed_size) else {
        return Ok((StatusCode::BAD_REQUEST, "size must be a positive integer").into_response());
    };
    let Some((att, thumb)) = thumbnails.get_or_generate(&id, size).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&thumb.mime_type).unwrap_or(OCTET_STREAM),
            ),
            (header::CACHE_CONTROL, IMMUTABLE_CACHE),
            (header::ETAG, etag(&format!("{}-{size}", att.sha256))?),
        ],
        thumb.data,
    )
        .into_response())
}

/// A strong ETag: `tag` in double quotes.
fn etag(tag: &str) -> anyhow::Result<HeaderValue> {
    HeaderValue::from_str(&format!("\"{tag}\"")).with_context(|| format!("etag for {tag:?}"))
}

/// `inline; filename="…"`, plus an RFC 6266 `filename*` when the title is not
/// ASCII. Quotes and backslashes are dropped and each run of control
/// characters becomes one space, so a hostile title can neither inject a
/// header nor break out of the quoted string.
fn content_disposition(title: &str) -> HeaderValue {
    let mut name = String::with_capacity(title.len());
    let mut after_control = false;
    for c in title.chars() {
        if c.is_control() {
            after_control = true;
        } else if c != '"' && c != '\\' {
            if after_control && !name.is_empty() {
                name.push(' ');
            }
            after_control = false;
            name.push(c);
        }
    }
    let name = match name.trim() {
        "" => "attachment",
        trimmed => trimmed,
    };
    let value = if name.is_ascii() {
        format!("inline; filename=\"{name}\"")
    } else {
        let fallback: String = name
            .chars()
            .map(|c| if c.is_ascii() { c } else { '_' })
            .collect();
        format!(
            "inline; filename=\"{fallback}\"; filename*=UTF-8''{}",
            percent_encode(name)
        )
    };
    // Every byte left is visible ASCII or a space, so this cannot fail.
    HeaderValue::from_str(&value).unwrap_or(INLINE)
}

/// RFC 5987 `value-chars`: `attr-char`s verbatim, every other byte as `%XX`.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            // Writing to a `String` cannot fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disposition(title: &str) -> String {
        content_disposition(title).to_str().unwrap().to_owned()
    }

    #[test]
    fn plain_titles_pass_through() {
        assert_eq!(
            disposition("manual.pdf"),
            r#"inline; filename="manual.pdf""#
        );
    }

    #[test]
    fn hostile_titles_are_neutralised() {
        assert_eq!(disposition("a\\b\"c\r\n\td"), r#"inline; filename="abc d""#);
        assert_eq!(disposition("\r\n\"\""), r#"inline; filename="attachment""#);
        assert_eq!(disposition(""), r#"inline; filename="attachment""#);
    }

    #[test]
    fn non_ascii_titles_get_an_extended_parameter() {
        assert_eq!(
            disposition("naïve \"x\".jpg"),
            "inline; filename=\"na_ve x.jpg\"; filename*=UTF-8''na%C3%AFve%20x.jpg"
        );
    }
}
