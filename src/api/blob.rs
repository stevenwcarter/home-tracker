//! Serving a stored original or one of its thumbnails over HTTP, shared by
//! the attachment and ingest photo routes so their behaviour cannot drift.
//!
//! Both responses are immutably cacheable: the SPA's URLs carry a version
//! (`?v=`), and an original's bytes are addressed by their sha256, which is
//! also the ETag.
//!
//! Both are also hardened: the bytes are user-supplied but served from the
//! app's origin, so `X-Content-Type-Options: nosniff` stops the browser
//! guessing a more dangerous type, and `Content-Security-Policy: sandbox`
//! makes an HTML or SVG original opened directly a sandboxed document that
//! cannot run script on the app origin.

use std::fmt::{self, Write as _};
use std::io::{self, ErrorKind};
use std::path::Path;

use anyhow::{Context, anyhow};
use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use tracing::Instrument;

use crate::api::{AppError, IMMUTABLE_CACHE};
use crate::svc::attachment;
use crate::svc::blob::Blob;
use crate::svc::thumbnail::{ThumbSize, allowed_size, is_thumbnailable};
use crate::svc::thumbnail_service::ThumbnailService;

/// Which kind of row a served blob belongs to, for log lines and error
/// messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobOwner {
    Attachment,
    IngestPhoto,
}

impl fmt::Display for BlobOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Attachment => "attachment",
            Self::IngestPhoto => "ingest photo",
        })
    }
}

const OCTET_STREAM: HeaderValue = HeaderValue::from_static("application/octet-stream");
const INLINE: HeaderValue = HeaderValue::from_static("inline");
const NOSNIFF: HeaderValue = HeaderValue::from_static("nosniff");
const SANDBOX: HeaderValue = HeaderValue::from_static("sandbox");

/// A thumbnail route's `{size}` that is not a positive integer: 400.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadThumbSize;

impl IntoResponse for BadThumbSize {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, "size must be a positive integer").into_response()
    }
}

/// The `{size}` path segment of a thumbnail route as an allowed size.
pub fn thumb_size(raw: &str) -> Result<ThumbSize, BadThumbSize> {
    raw.parse::<i64>()
        .ok()
        .and_then(allowed_size)
        .ok_or(BadThumbSize)
}

/// `blob`'s original from under `data_dir`, downloaded as `title`: 404 when
/// the file is missing. `owner` and `id` name the row in logs and in a 500's
/// message, which never carries a path.
pub async fn serve_original(
    data_dir: &Path,
    blob: &Blob,
    title: &str,
    owner: BlobOwner,
    id: &str,
) -> Result<Response, AppError> {
    let path = attachment::original_path(data_dir, &blob.sha256);
    // Paths go to the log only: error messages reach clients.
    let unreadable = |err: io::Error| {
        tracing::error!(%id, path = %path.display(), %err, "could not read original");
        anyhow!(err).context(format!("reading the original of {owner} {id:?}"))
    };
    let file = match File::open(&path).await {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            tracing::warn!(%id, path = %path.display(), "original file missing");
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
        Err(err) => return Err(unreadable(err).into()),
    };
    // The file's own length, not the row's size: a stale row must not make
    // the response lie about its body.
    let len = file.metadata().await.map_err(unreadable)?.len();
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&blob.mime_type).unwrap_or(OCTET_STREAM),
            ),
            (header::CONTENT_LENGTH, HeaderValue::from(len)),
            (header::CONTENT_DISPOSITION, content_disposition(title)),
            (header::CACHE_CONTROL, IMMUTABLE_CACHE),
            (header::ETAG, etag(&blob.sha256)?),
            (header::X_CONTENT_TYPE_OPTIONS, NOSNIFF),
            (header::CONTENT_SECURITY_POLICY, SANDBOX),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response())
}

/// `blob`'s `size` thumbnail, generated on first request: 404 for a
/// non-image or an original that cannot be thumbnailed. `owner` and `id`
/// name the row as in [`serve_original`].
pub async fn serve_thumb(
    thumbnails: &ThumbnailService,
    blob: &Blob,
    size: ThumbSize,
    owner: BlobOwner,
    id: &str,
) -> Result<Response, AppError> {
    if !is_thumbnailable(&blob.mime_type) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    // The span puts the row's id on the service's log lines, which only know
    // the blob.
    let thumb = thumbnails
        .get_or_generate(blob, size)
        .instrument(tracing::info_span!("thumb", %id))
        .await
        .with_context(|| format!("thumbnail of {owner} {id:?}"))?;
    let Some(thumb) = thumb else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&thumb.mime_type).unwrap_or(OCTET_STREAM),
            ),
            (header::CACHE_CONTROL, IMMUTABLE_CACHE),
            (
                header::ETAG,
                etag(&format!("{}-{}", blob.sha256, size.get()))?,
            ),
            (header::X_CONTENT_TYPE_OPTIONS, NOSNIFF),
            (header::CONTENT_SECURITY_POLICY, SANDBOX),
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

    #[test]
    fn thumb_sizes_round_up_and_refuse_non_positive_integers() {
        assert_eq!(thumb_size("301").unwrap().get(), 500);
        for bad in ["0", "-1", "big", ""] {
            assert_eq!(thumb_size(bad), Err(BadThumbSize), "{bad:?}");
        }
        assert_eq!(
            BadThumbSize.into_response().status(),
            StatusCode::BAD_REQUEST
        );
    }
}
