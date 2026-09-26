//! The AI settings surface through `/graphql`: defaults, the write-only key,
//! environment overrides, validation, the endpoint-host rule for the key, the
//! connection test (through the fake and through the real client), and the
//! write gate.

// Each test crate uses a different subset of the shared test support.
#[allow(dead_code)]
mod support;

use std::sync::Arc;

use axum::http::StatusCode;
use axum_test::TestServer;
use home_tracker::ai::AiState;
use home_tracker::ai::client::{AiError, ChatRequest, ContentPart, Message, Role as ChatRole};
use home_tracker::ai::env::AiEnv;
use home_tracker::ai::fake::FakeAiClient;
use home_tracker::ai::openai::OpenAiClient;
use home_tracker::db::TestDb;
use home_tracker::graphql::context::{Actor, Role};
use home_tracker::routes::{app_with_actor, app_with_ai};
use home_tracker::svc::ai_settings;
use serde_json::{Value, json};
use support::openai_stub::OpenAiStub;
use tempfile::TempDir;

const KEY: &str = "sk-test-0123456789abcdef";
/// [`KEY`] as OpenAI echoes a rejected key: the first 8 and last 4 characters.
const PARTIAL_KEY: &str = "sk-test-************cdef";
const ELSEWHERE: &str = "https://elsewhere.example/v1";

const SETTINGS: &str = "{ aiSettings {
    baseUrl visionModel synthesisModel extraInstructions hasApiKey fromEnvironment
} }";

const UPDATE: &str = "mutation($input: AiSettingsInput!) {
    updateAiSettings(input: $input) {
        baseUrl visionModel synthesisModel extraInstructions hasApiKey fromEnvironment
    }
}";

const TEST_CONNECTION: &str = "mutation { testAiConnection { ok message latencyMs } }";

/// A server with AI state `ai`; the `TempDir` is its data dir.
fn server_with_ai(ai: AiState) -> (TestServer, TestDb, TempDir) {
    let db = TestDb::new();
    let data = tempfile::tempdir().unwrap();
    let server = TestServer::new(app_with_ai(
        db.pool.clone(),
        data.path().to_path_buf(),
        Arc::new(ai),
    ));
    (server, db, data)
}

/// A server whose AI environment is `env`, with the model client disabled.
fn server_with(env: AiEnv) -> (TestServer, TestDb, TempDir) {
    server_with_ai(AiState {
        env,
        ..AiState::disabled()
    })
}

/// A server whose model client is `fake`, with no environment overrides.
fn server_with_fake(fake: &Arc<FakeAiClient>) -> (TestServer, TestDb, TempDir) {
    server_with_ai(AiState {
        env: AiEnv::none(),
        client: Arc::clone(fake) as _,
    })
}

fn server() -> (TestServer, TestDb, TempDir) {
    server_with(AiEnv::none())
}

/// A server with the real [`OpenAiClient`] (no retries) and no environment
/// overrides, for end-to-end tests against an [`OpenAiStub`].
fn server_with_real_client() -> (TestServer, TestDb, TempDir) {
    server_with_ai(AiState {
        env: AiEnv::none(),
        client: Arc::new(OpenAiClient::new().with_backoff(vec![])),
    })
}

async fn post(server: &TestServer, doc: &str, vars: Value) -> Value {
    let r = server
        .post("/graphql")
        .json(&json!({ "query": doc, "variables": vars }))
        .await;
    r.assert_status_ok();
    r.json()
}

/// The whole `aiSettings` response body (so callers can search all of it).
async fn query_body(server: &TestServer) -> Value {
    let body = post(server, SETTINGS, json!({})).await;
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    body
}

/// `updateAiSettings` with `input`, which must succeed; the whole body.
async fn update_body(server: &TestServer, input: Value) -> Value {
    let body = post(server, UPDATE, json!({ "input": input })).await;
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    body
}

async fn update(server: &TestServer, input: Value) -> Value {
    update_body(server, input).await["data"]["updateAiSettings"].clone()
}

/// `updateAiSettings` with `input`, which must fail; the whole body and its
/// first error message.
async fn update_refused(server: &TestServer, input: Value) -> (Value, String) {
    let body = post(server, UPDATE, json!({ "input": input })).await;
    let message = body["errors"][0]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("expected an error: {body}"))
        .to_owned();
    (body, message)
}

/// `updateAiSettings` with `input`, which must fail; its first error message.
async fn update_err(server: &TestServer, input: Value) -> String {
    update_refused(server, input).await.1
}

/// A valid input on the defaults, with `extra` merged over it.
fn input(extra: Value) -> Value {
    let mut base = json!({
        "baseUrl": "https://api.openai.com/v1",
        "visionModel": "gpt-5-mini",
        "synthesisModel": "gpt-5-mini",
    });
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

/// The key `svc::ai_settings::config` resolves, read straight from the database.
fn stored_key(db: &TestDb) -> Option<String> {
    let mut conn = db.pool.get().unwrap();
    ai_settings::config(&mut conn, &AiEnv::none())
        .unwrap()
        .map(|c| c.api_key)
}

#[tokio::test]
async fn defaults_are_reported_and_no_key_is_set() {
    let (server, _db, _data) = server();
    let body = query_body(&server).await;
    assert_eq!(
        body["data"]["aiSettings"],
        json!({
            "baseUrl": "https://api.openai.com/v1",
            "visionModel": "gpt-5-mini",
            "synthesisModel": "gpt-5-mini",
            "extraInstructions": null,
            "hasApiKey": false,
            "fromEnvironment": [],
        })
    );
}

#[tokio::test]
async fn saving_a_key_is_write_only() {
    let (server, db, _data) = server();

    let body = update_body(&server, input(json!({ "apiKey": format!("  {KEY}  ") }))).await;
    assert_eq!(body["data"]["updateAiSettings"]["hasApiKey"], json!(true));
    assert!(!body.to_string().contains(KEY), "key leaked: {body}");

    let body = query_body(&server).await;
    assert_eq!(body["data"]["aiSettings"]["hasApiKey"], json!(true));
    assert!(!body.to_string().contains(KEY), "key leaked: {body}");

    // Stored trimmed, where the server (and only the server) can read it.
    assert_eq!(stored_key(&db).as_deref(), Some(KEY));
}

#[tokio::test]
async fn an_empty_key_clears_it() {
    let (server, db, _data) = server();
    update(&server, input(json!({ "apiKey": KEY }))).await;

    let settings = update(&server, input(json!({ "apiKey": "" }))).await;
    assert_eq!(settings["hasApiKey"], json!(false));
    assert_eq!(stored_key(&db), None);
}

#[tokio::test]
async fn a_null_key_keeps_it() {
    let (server, db, _data) = server();
    update(&server, input(json!({ "apiKey": KEY }))).await;

    let settings = update(
        &server,
        input(json!({ "apiKey": null, "visionModel": "gpt-5" })),
    )
    .await;
    assert_eq!(settings["hasApiKey"], json!(true));
    assert_eq!(settings["visionModel"], json!("gpt-5"));
    // Omitted keeps it too.
    let settings = update(&server, input(json!({}))).await;
    assert_eq!(settings["hasApiKey"], json!(true));
    assert_eq!(stored_key(&db).as_deref(), Some(KEY));
}

#[tokio::test]
async fn env_key_wins_and_is_reported() {
    let (server, db, _data) = server_with(AiEnv {
        api_key: Some("env-key".to_owned()),
        ..AiEnv::none()
    });

    let body = query_body(&server).await;
    let settings = &body["data"]["aiSettings"];
    assert_eq!(settings["fromEnvironment"], json!(["OPENAI_API_KEY"]));
    assert_eq!(settings["hasApiKey"], json!(true));
    assert!(!body.to_string().contains("env-key"), "key leaked: {body}");

    let (body, err) = update_refused(&server, input(json!({ "apiKey": KEY }))).await;
    assert!(
        err.contains("OPENAI_API_KEY is set in the environment"),
        "{err}"
    );
    let shown = body.to_string();
    assert!(
        !shown.contains(KEY) && !shown.contains("env-key"),
        "key leaked: {shown}"
    );
    // A clear is a change of that field too.
    let err = update_err(&server, input(json!({ "apiKey": "" }))).await;
    assert!(err.contains("OPENAI_API_KEY"), "{err}");
    assert_eq!(stored_key(&db), None, "the refused update wrote nothing");

    // The other fields still save.
    let settings = update(
        &server,
        input(json!({ "visionModel": "gpt-5", "synthesisModel": "gpt-5-large" })),
    )
    .await;
    assert_eq!(settings["visionModel"], json!("gpt-5"));
    assert_eq!(settings["synthesisModel"], json!("gpt-5-large"));
    assert_eq!(settings["fromEnvironment"], json!(["OPENAI_API_KEY"]));
}

#[tokio::test]
async fn env_base_url_wins_and_is_reported() {
    let (server, _db, _data) = server_with(AiEnv {
        base_url: Some("http://llm.lan:8080/v1/".to_owned()),
        ..AiEnv::none()
    });

    let settings = query_body(&server).await["data"]["aiSettings"].clone();
    assert_eq!(settings["fromEnvironment"], json!(["OPENAI_BASE_URL"]));
    assert_eq!(settings["baseUrl"], json!("http://llm.lan:8080/v1"));
    assert_eq!(settings["hasApiKey"], json!(false));

    let err = update_err(
        &server,
        input(json!({ "baseUrl": "https://elsewhere.example/v1" })),
    )
    .await;
    assert!(
        err.contains("OPENAI_BASE_URL is set in the environment"),
        "{err}"
    );

    // Sending back the value the form was shown is not a change.
    let settings = update(
        &server,
        input(json!({ "baseUrl": "http://llm.lan:8080/v1", "visionModel": "llava" })),
    )
    .await;
    assert_eq!(settings["visionModel"], json!("llava"));
    assert_eq!(settings["baseUrl"], json!("http://llm.lan:8080/v1"));
}

#[tokio::test]
async fn base_url_is_normalised() {
    let (server, _db, _data) = server();

    let settings = update(&server, input(json!({ "baseUrl": "  https://host/v1/  " }))).await;
    assert_eq!(settings["baseUrl"], json!("https://host/v1"));

    for bad in ["ftp://x", "not a url", "", "https://"] {
        let err = update_err(&server, input(json!({ "baseUrl": bad }))).await;
        assert!(err.contains("base URL"), "{bad:?}: {err}");
    }
    let settings = query_body(&server).await["data"]["aiSettings"].clone();
    assert_eq!(settings["baseUrl"], json!("https://host/v1"));
}

#[tokio::test]
async fn a_base_url_with_credentials_is_refused() {
    let (server, _db, _data) = server();

    let err = update_err(
        &server,
        input(json!({ "baseUrl": format!("https://user:{KEY}@host/v1") })),
    )
    .await;
    assert!(err.contains("user name or password"), "{err}");
    assert!(!err.contains(KEY), "key leaked: {err}");
    let settings = query_body(&server).await["data"]["aiSettings"].clone();
    assert_eq!(settings["baseUrl"], json!("https://api.openai.com/v1"));
}

#[tokio::test]
async fn a_base_url_with_a_query_or_fragment_is_refused() {
    let (server, _db, _data) = server();

    for bad in ["https://host/v1?api-version=1", "https://host/v1#frag"] {
        let err = update_err(&server, input(json!({ "baseUrl": bad }))).await;
        assert!(err.contains("query string or fragment"), "{bad:?}: {err}");
    }
    let settings = query_body(&server).await["data"]["aiSettings"].clone();
    assert_eq!(settings["baseUrl"], json!("https://api.openai.com/v1"));
}

#[tokio::test]
async fn an_endpoint_host_change_without_a_new_key_clears_the_key() {
    let (server, db, _data) = server();

    // A different host, scheme or port each send the key somewhere new.
    for elsewhere in [
        ELSEWHERE,
        "http://api.openai.com/v1",
        "https://api.openai.com:8443/v1",
    ] {
        update(&server, input(json!({ "apiKey": KEY }))).await;
        let settings = update(&server, input(json!({ "baseUrl": elsewhere }))).await;
        assert_eq!(settings["hasApiKey"], json!(false), "{elsewhere}");
        assert_eq!(settings["baseUrl"], json!(elsewhere));
        assert_eq!(stored_key(&db), None, "{elsewhere}");
    }
}

#[tokio::test]
async fn an_endpoint_host_change_with_a_new_key_keeps_the_new_key() {
    let (server, db, _data) = server();
    update(&server, input(json!({ "apiKey": KEY }))).await;

    let settings = update(
        &server,
        input(json!({ "baseUrl": ELSEWHERE, "apiKey": "sk-elsewhere" })),
    )
    .await;
    assert_eq!(settings["hasApiKey"], json!(true));
    assert_eq!(stored_key(&db).as_deref(), Some("sk-elsewhere"));
}

#[tokio::test]
async fn a_same_host_endpoint_change_keeps_the_key() {
    let (server, db, _data) = server();
    update(&server, input(json!({ "apiKey": KEY }))).await;

    // The path, host case and an explicit default port are not a new host.
    for same in [
        "https://api.openai.com/openai/v1",
        "https://API.OpenAI.com/v1",
        "https://api.openai.com:443/v1",
    ] {
        let settings = update(&server, input(json!({ "baseUrl": same }))).await;
        assert_eq!(settings["hasApiKey"], json!(true), "{same}");
        assert_eq!(stored_key(&db).as_deref(), Some(KEY), "{same}");
    }
}

#[tokio::test]
async fn an_endpoint_host_change_is_refused_while_the_key_is_from_the_environment() {
    let (server, _db, _data) = server_with(AiEnv {
        api_key: Some("env-key".to_owned()),
        ..AiEnv::none()
    });

    let (body, err) = update_refused(&server, input(json!({ "baseUrl": ELSEWHERE }))).await;
    assert!(err.contains("OPENAI_API_KEY"), "{err}");
    assert!(err.contains("endpoint host"), "{err}");
    assert!(!body.to_string().contains("env-key"), "key leaked: {body}");
    let settings = query_body(&server).await["data"]["aiSettings"].clone();
    assert_eq!(settings["baseUrl"], json!("https://api.openai.com/v1"));

    // The same host on another path is fine.
    let settings = update(
        &server,
        input(json!({ "baseUrl": "https://api.openai.com/openai/v1" })),
    )
    .await;
    assert_eq!(
        settings["baseUrl"],
        json!("https://api.openai.com/openai/v1")
    );
    assert_eq!(settings["hasApiKey"], json!(true));
}

#[tokio::test]
async fn blank_models_are_refused() {
    let (server, _db, _data) = server();

    let err = update_err(&server, input(json!({ "visionModel": "  " }))).await;
    assert!(err.contains("vision model must not be blank"), "{err}");
    let err = update_err(&server, input(json!({ "synthesisModel": "" }))).await;
    assert!(err.contains("synthesis model must not be blank"), "{err}");

    let settings = update(&server, input(json!({ "visionModel": "  gpt-5  " }))).await;
    assert_eq!(settings["visionModel"], json!("gpt-5"));
}

#[tokio::test]
async fn extra_instructions_over_4000_chars_are_refused() {
    let (server, _db, _data) = server();

    // Counted in characters, not bytes: 4000 two-byte characters fit.
    let at_limit = "é".repeat(4000);
    let settings = update(&server, input(json!({ "extraInstructions": at_limit }))).await;
    assert_eq!(settings["extraInstructions"], json!(at_limit));

    let err = update_err(
        &server,
        input(json!({ "extraInstructions": "x".repeat(4001) })),
    )
    .await;
    assert!(err.contains("4000"), "{err}");

    let settings = update(&server, input(json!({ "extraInstructions": "  \n " }))).await;
    assert_eq!(settings["extraInstructions"], Value::Null);
}

/// `testAiConnection`, which must not error; its result object.
async fn test_connection(server: &TestServer) -> Value {
    let body = post(server, TEST_CONNECTION, json!({})).await;
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    body["data"]["testAiConnection"].clone()
}

#[tokio::test]
async fn test_connection_reports_ok_via_the_fake() {
    let fake = Arc::new(FakeAiClient::new());
    let (server, _db, _data) = server_with_fake(&fake);
    update(
        &server,
        input(json!({ "apiKey": KEY, "synthesisModel": "synth-1" })),
    )
    .await;
    fake.push(Ok("OK".to_owned()));

    let result = test_connection(&server).await;
    assert_eq!(result["ok"], json!(true), "{result}");
    let message = result["message"].as_str().unwrap();
    assert!(
        message.starts_with("Connected: synth-1 answered in ") && message.ends_with(" ms"),
        "{message}"
    );
    assert!(result["latencyMs"].as_i64().unwrap() >= 0, "{result}");

    assert_eq!(
        fake.requests(),
        [ChatRequest {
            model: "synth-1".to_owned(),
            messages: vec![Message {
                role: ChatRole::User,
                content: vec![ContentPart::Text(
                    "Reply with the single word OK.".to_owned()
                )],
            }],
            response_format: None,
            max_tokens: Some(8),
        }]
    );
}

#[tokio::test]
async fn test_connection_reports_the_provider_error() {
    let fake = Arc::new(FakeAiClient::new());
    let (server, _db, _data) = server_with_fake(&fake);
    update(&server, input(json!({ "apiKey": KEY }))).await;
    let error = AiError::Status {
        status: 401,
        snippet: "invalid key".to_owned(),
    };
    fake.push(Err(error.clone()));

    let result = test_connection(&server).await;
    assert_eq!(result["ok"], json!(false), "{result}");
    assert_eq!(result["message"], json!(error.to_string()));
    assert!(!result.to_string().contains(KEY), "key leaked: {result}");
}

/// A completion from the stub answering the connection test.
fn stub_completion() -> (StatusCode, Value) {
    (
        StatusCode::OK,
        json!({
            "model": "stub-model",
            "choices": [{ "message": { "role": "assistant", "content": "OK" } }],
        }),
    )
}

#[tokio::test]
async fn test_connection_reaches_the_saved_endpoint_through_the_real_client() {
    let stub = OpenAiStub::start(|_, _| stub_completion()).await;
    let (server, _db, _data) = server_with_real_client();
    update(
        &server,
        input(json!({ "baseUrl": format!("{}/v1", stub.base_url), "apiKey": KEY })),
    )
    .await;

    let result = test_connection(&server).await;
    assert_eq!(result["ok"], json!(true), "{result}");
    let message = result["message"].as_str().unwrap();
    assert!(
        message.starts_with("Connected: stub-model answered in "),
        "{message}"
    );

    let requests = stub.requests();
    assert_eq!(requests.len(), 1);
    let sent = &requests[0];
    assert_eq!(sent.path, "/v1/chat/completions");
    assert_eq!(
        sent.headers["authorization"].to_str().unwrap(),
        format!("Bearer {KEY}")
    );
    assert_eq!(sent.body["model"], json!("gpt-5-mini"));
    assert_eq!(sent.body["max_completion_tokens"], json!(8));
}

#[tokio::test]
async fn test_connection_masks_a_provider_echo_of_the_key() {
    let stub = OpenAiStub::start(|_, _| {
        (
            StatusCode::UNAUTHORIZED,
            json!({ "error": { "message": format!(
                "Incorrect API key provided: {PARTIAL_KEY}. You sent: Bearer {KEY}"
            ) } }),
        )
    })
    .await;
    let (server, _db, _data) = server_with_real_client();
    update(
        &server,
        input(json!({ "baseUrl": format!("{}/v1", stub.base_url), "apiKey": KEY })),
    )
    .await;

    let body = post(&server, TEST_CONNECTION, json!({})).await;
    assert_eq!(
        body["data"]["testAiConnection"]["ok"],
        json!(false),
        "{body}"
    );
    let shown = body.to_string();
    assert!(shown.contains("Incorrect API key provided"), "{shown}");
    assert!(!shown.contains(KEY), "key leaked: {shown}");
    assert!(!shown.contains(PARTIAL_KEY), "partial key leaked: {shown}");
}

#[tokio::test]
async fn test_connection_reports_not_configured() {
    let fake = Arc::new(FakeAiClient::new());
    let (server, _db, _data) = server_with_fake(&fake);

    let result = test_connection(&server).await;
    assert_eq!(
        result,
        json!({
            "ok": false,
            "message": "AI is not configured: add an API key first",
            "latencyMs": 0,
        })
    );
    assert!(fake.requests().is_empty(), "no call without a key");
}

#[tokio::test]
async fn read_only_actor_is_forbidden() {
    let db = TestDb::new();
    let data = tempfile::tempdir().unwrap();
    let reader = Actor::User {
        id: "u".to_owned(),
        role: Role::ReadOnly,
    };
    let server = TestServer::new(app_with_actor(
        db.pool.clone(),
        data.path().to_path_buf(),
        reader,
    ));

    let err = update_err(
        &server,
        input(json!({ "apiKey": KEY, "visionModel": "gpt-5" })),
    )
    .await;
    assert!(err.contains("Forbidden"), "{err}");
    assert_eq!(stored_key(&db), None);
    let body = post(&server, TEST_CONNECTION, json!({})).await;
    let err = body["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(err.contains("Forbidden"), "{body}");
    // Reading is still allowed, and shows nothing changed.
    let settings = query_body(&server).await["data"]["aiSettings"].clone();
    assert_eq!(settings["visionModel"], json!("gpt-5-mini"));
}
