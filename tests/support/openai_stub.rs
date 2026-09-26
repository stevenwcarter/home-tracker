//! A fake OpenAI-compatible server on `127.0.0.1:0` for client tests: it
//! records every request and answers from a caller-supplied handler.
//! Test-only code: failures panic.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::time;

/// One request as the stub saw it.
#[derive(Debug, Clone)]
pub struct Captured {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    /// The body as JSON, or `Value::Null` when it is not JSON.
    pub body: Value,
}

/// Answers call number `index` (from 0) whose JSON body is the second argument.
type Handler = dyn Fn(usize, &Value) -> (StatusCode, Value) + Send + Sync;

struct Shared {
    handler: Box<Handler>,
    delay: Duration,
    requests: Mutex<Vec<Captured>>,
}

/// A running stub; it serves until the test's runtime shuts down.
pub struct OpenAiStub {
    /// `http://127.0.0.1:<port>`, without a trailing slash.
    pub base_url: String,
    shared: Arc<Shared>,
}

impl OpenAiStub {
    /// Serves every path with `handler`, answering at once.
    pub async fn start(
        handler: impl Fn(usize, &Value) -> (StatusCode, Value) + Send + Sync + 'static,
    ) -> Self {
        Self::start_delayed(Duration::ZERO, handler).await
    }

    /// Like [`Self::start`], but every answer waits `delay` first.
    pub async fn start_delayed(
        delay: Duration,
        handler: impl Fn(usize, &Value) -> (StatusCode, Value) + Send + Sync + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            handler: Box::new(handler),
            delay,
            requests: Mutex::new(Vec::new()),
        });
        let app = Router::new()
            .fallback(answer)
            .with_state(Arc::clone(&shared));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            base_url: format!("http://{addr}"),
            shared,
        }
    }

    /// Every request received so far, in arrival order.
    pub fn requests(&self) -> Vec<Captured> {
        self.shared.requests.lock().unwrap().clone()
    }
}

async fn answer(
    State(shared): State<Arc<Shared>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let index = {
        let mut requests = shared.requests.lock().unwrap();
        requests.push(Captured {
            method,
            path: uri.path().to_owned(),
            headers,
            body: body.clone(),
        });
        requests.len() - 1
    };
    let (status, reply) = (shared.handler)(index, &body);
    time::sleep(shared.delay).await;
    (status, Json(reply)).into_response()
}
