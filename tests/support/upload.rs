//! Multipart upload helpers shared by the attachment upload and the ingest
//! staging tests: forms, hand-built requests axum-test cannot shape, a body
//! that must never be read, and JSON error bodies. Test-only code: failures
//! panic.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use axum::body::{self, Body};
use axum::http::{Request, header};
use axum::response::Response;
use axum_test::TestResponse;
use axum_test::multipart::{MultipartForm, Part};
use home_tracker::graphql::context::{Actor, Role};
use serde_json::Value;
use tokio::io::{self, AsyncRead, ReadBuf};
use tokio_util::io::ReaderStream;

/// The boundary of every hand-built multipart body.
pub const BOUNDARY: &str = "home-tracker-test-boundary";

/// The `Content-Type` of a hand-built multipart body.
pub fn multipart_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

/// A form holding `bytes` as the `file` field, named `photo.jpg`.
pub fn photo_form(bytes: Vec<u8>) -> MultipartForm {
    MultipartForm::new().add_part(
        "file",
        Part::bytes(bytes)
            .file_name("photo.jpg")
            .mime_type("image/jpeg"),
    )
}

/// A multipart `POST` to `uri` whose body is `body`, streamed in 64 KiB
/// chunks with no `Content-Length`.
pub fn streamed_request(uri: &str, body: impl AsyncRead + Send + 'static) -> Request<Body> {
    Request::post(uri)
        .header(header::CONTENT_TYPE, multipart_type())
        .body(Body::from_stream(ReaderStream::with_capacity(
            body,
            64 * 1024,
        )))
        .unwrap()
}

/// A body that records being polled and never yields: a handler that reads
/// it hangs, and the flag says it tried.
pub struct Untouchable(pub Arc<AtomicBool>);

impl AsyncRead for Untouchable {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.0.store(true, Ordering::SeqCst);
        Poll::Pending
    }
}

/// A signed-in actor who may only read.
pub fn read_only() -> Actor {
    Actor::User {
        id: "u1".to_owned(),
        role: Role::ReadOnly,
    }
}

/// The JSON body of a response from a router called directly.
pub async fn json_body(response: Response) -> Value {
    let bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("not JSON ({err}): {bytes:?}"))
}

/// The `{ "error": … }` message of a JSON error response.
pub fn error_message(response: &TestResponse) -> String {
    let body: Value = response.json();
    body["error"]
        .as_str()
        .unwrap_or_else(|| panic!("expected a JSON error body: {body}"))
        .to_owned()
}
