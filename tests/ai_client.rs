//! `OpenAiClient` against a stub OpenAI server: the request shape, the
//! `json_schema -> json_object` fallback, retries, the timeout, response
//! parsing, and that the API key never reaches an error.

// Each test crate uses a different subset of the shared test support.
#[allow(dead_code)]
mod support;

use std::net::TcpListener;
use std::time::Duration;

use axum::http::StatusCode;
use home_tracker::ai::client::{
    AiClient, AiError, ChatRequest, ChatResponse, ContentPart, Detail, Message, ResponseFormat,
    Role, Usage,
};
use home_tracker::ai::openai::OpenAiClient;
use home_tracker::svc::ai_settings::AiConfig;
use serde_json::{Value, json};
use support::openai_stub::OpenAiStub;

const KEY: &str = "sk-test-secret-0123456789";

/// A config pointing at `stub`, with a trailing slash on the base URL.
fn config(stub: &OpenAiStub) -> AiConfig {
    AiConfig {
        base_url: format!("{}/v1/", stub.base_url),
        api_key: KEY.to_owned(),
        vision_model: "vision-model".to_owned(),
        synthesis_model: "synthesis-model".to_owned(),
        extra_instructions: None,
    }
}

/// No waiting between retries, so the retry tests run instantly.
fn client() -> OpenAiClient {
    OpenAiClient::new().with_backoff(vec![Duration::ZERO; 2])
}

fn request() -> ChatRequest {
    ChatRequest {
        model: "synthesis-model".to_owned(),
        messages: vec![Message {
            role: Role::User,
            content: vec![
                ContentPart::Text("Describe this.".to_owned()),
                ContentPart::ImageUrl {
                    url: "data:image/webp;base64,AAAA".to_owned(),
                    detail: Detail::Auto,
                },
            ],
        }],
        response_format: Some(ResponseFormat::JsonSchema {
            name: "item".to_owned(),
            schema: json!({ "type": "object" }),
        }),
        max_tokens: Some(100),
    }
}

/// A successful completion whose content is `content`.
fn completion(content: Value) -> Value {
    json!({
        "model": "synthesis-model-2026",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": content } }],
    })
}

fn ok(content: &str) -> (StatusCode, Value) {
    (StatusCode::OK, completion(json!(content)))
}

#[tokio::test]
async fn sends_the_documented_request_shape() {
    let stub = OpenAiStub::start(|_, _| ok("{}")).await;

    let response = client().chat(&config(&stub), request()).await.unwrap();
    assert_eq!(response.content, "{}");
    assert_eq!(response.model, "synthesis-model-2026");

    let requests = stub.requests();
    assert_eq!(requests.len(), 1);
    let sent = &requests[0];
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.path, "/v1/chat/completions");
    assert_eq!(
        sent.headers["authorization"].to_str().unwrap(),
        format!("Bearer {KEY}")
    );
    assert_eq!(
        sent.body,
        json!({
            "model": "synthesis-model",
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "Describe this." },
                    {
                        "type": "image_url",
                        "image_url": { "url": "data:image/webp;base64,AAAA", "detail": "auto" },
                    },
                ],
            }],
            "response_format": {
                "type": "json_schema",
                "json_schema": { "name": "item", "schema": { "type": "object" }, "strict": true },
            },
            // Not `max_tokens`, which OpenAI's reasoning models refuse.
            "max_completion_tokens": 100,
        })
    );
}

#[tokio::test]
async fn falls_back_to_json_object_when_json_schema_is_rejected() {
    let stub = OpenAiStub::start(|index, _| match index {
        0 => (
            StatusCode::BAD_REQUEST,
            json!({ "error": { "message": "Invalid parameter: 'response_format' of type 'json_schema' is not supported with this model." } }),
        ),
        _ => ok(r#"{"name":"Mouse"}"#),
    })
    .await;

    let response = client().chat(&config(&stub), request()).await.unwrap();
    assert_eq!(response.content, r#"{"name":"Mouse"}"#);

    let requests = stub.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].body["response_format"]["type"],
        json!("json_schema")
    );
    assert_eq!(
        requests[1].body["response_format"],
        json!({ "type": "json_object" })
    );
    assert_eq!(requests[1].path, "/v1/chat/completions");
}

#[tokio::test]
async fn falls_back_at_most_once() {
    let stub = OpenAiStub::start(|_, _| {
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": { "message": "bad response_format" } }),
        )
    })
    .await;

    let err = client().chat(&config(&stub), request()).await.unwrap_err();
    assert!(
        matches!(err, AiError::Status { status: 400, .. }),
        "{err:?}"
    );
    assert_eq!(stub.requests().len(), 2);
}

#[tokio::test]
async fn a_400_about_something_else_is_not_retried() {
    let stub = OpenAiStub::start(|_, _| {
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": { "message": "context too long" } }),
        )
    })
    .await;

    let err = client().chat(&config(&stub), request()).await.unwrap_err();
    assert!(
        matches!(err, AiError::Status { status: 400, .. }),
        "{err:?}"
    );
    assert_eq!(stub.requests().len(), 1);
}

#[tokio::test]
async fn retries_on_429_then_succeeds() {
    let stub = OpenAiStub::start(|index, _| match index {
        0 | 1 => (
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": { "message": "slow down" } }),
        ),
        _ => ok("fine"),
    })
    .await;

    let response = client().chat(&config(&stub), request()).await.unwrap();
    assert_eq!(response.content, "fine");
    assert_eq!(stub.requests().len(), 3);
}

#[tokio::test]
async fn gives_up_after_the_retries_on_500() {
    let stub = OpenAiStub::start(|_, _| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": { "message": "boom" } }),
        )
    })
    .await;

    let err = client().chat(&config(&stub), request()).await.unwrap_err();
    match &err {
        AiError::Status { status, snippet } => {
            assert_eq!(*status, 500);
            assert!(snippet.contains("boom"), "{snippet}");
        }
        other => panic!("expected a 500 status error, got {other:?}"),
    }
    assert_eq!(stub.requests().len(), 3);
}

#[tokio::test]
async fn does_not_retry_a_401() {
    let stub = OpenAiStub::start(|_, _| {
        (
            StatusCode::UNAUTHORIZED,
            json!({ "error": { "message": format!("Incorrect API key provided: Bearer {KEY}") } }),
        )
    })
    .await;

    let err = client().chat(&config(&stub), request()).await.unwrap_err();
    assert_eq!(stub.requests().len(), 1);
    let AiError::Status { status, snippet } = &err else {
        panic!("expected a status error, got {err:?}");
    };
    assert_eq!(*status, 401);
    assert!(snippet.contains("Incorrect API key provided"), "{snippet}");
    assert!(snippet.contains("***"), "{snippet}");
    for shown in [snippet.clone(), err.to_string(), format!("{err:?}")] {
        assert!(!shown.contains(KEY), "key leaked: {shown}");
    }
}

#[tokio::test]
async fn error_snippets_are_truncated_to_500_chars() {
    let stub = OpenAiStub::start(|_, _| {
        (
            StatusCode::FORBIDDEN,
            json!({ "error": { "message": "é".repeat(2000) } }),
        )
    })
    .await;

    let err = client().chat(&config(&stub), request()).await.unwrap_err();
    let AiError::Status { snippet, .. } = err else {
        panic!("expected a status error, got {err:?}");
    };
    assert_eq!(snippet.chars().count(), 500);
}

#[tokio::test]
async fn times_out() {
    let stub = OpenAiStub::start_delayed(Duration::from_secs(1), |_, _| ok("late")).await;

    let err = client()
        .with_timeout(Duration::from_millis(200))
        .chat(&config(&stub), request())
        .await
        .unwrap_err();
    assert_eq!(err, AiError::Timeout);
    assert_eq!(stub.requests().len(), 1, "a timeout is not retried");
}

#[tokio::test]
async fn parses_usage_and_content_parts_array() {
    let stub = OpenAiStub::start(|_, _| {
        let mut body = completion(json!([
            { "type": "text", "text": "{\"name\":" },
            { "type": "text", "text": "\"Mouse\"}" },
        ]));
        body["usage"] =
            json!({ "prompt_tokens": 812, "completion_tokens": 64, "total_tokens": 876 });
        (StatusCode::OK, body)
    })
    .await;

    let response = client().chat(&config(&stub), request()).await.unwrap();
    assert_eq!(
        response,
        ChatResponse {
            content: r#"{"name":"Mouse"}"#.to_owned(),
            usage: Some(Usage {
                prompt_tokens: 812,
                completion_tokens: 64,
            }),
            model: "synthesis-model-2026".to_owned(),
        }
    );
}

#[tokio::test]
async fn a_completion_without_content_is_malformed() {
    let stub = OpenAiStub::start(|_, _| (StatusCode::OK, json!({ "choices": [] }))).await;

    let err = client().chat(&config(&stub), request()).await.unwrap_err();
    assert!(matches!(err, AiError::Malformed(_)), "{err:?}");
}

#[tokio::test]
async fn an_unreachable_provider_is_a_transport_error() {
    // Bind a port, then free it, so nothing is listening there.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let config = AiConfig {
        base_url: format!("http://{addr}/v1"),
        api_key: KEY.to_owned(),
        vision_model: "v".to_owned(),
        synthesis_model: "s".to_owned(),
        extra_instructions: None,
    };

    let err = client().chat(&config, request()).await.unwrap_err();
    assert!(matches!(err, AiError::Transport(_)), "{err:?}");
    assert!(!err.to_string().contains(KEY), "{err}");
}
