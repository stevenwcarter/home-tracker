//! [`AiClient`] over an OpenAI-compatible Chat Completions endpoint.
//!
//! One call is `POST {base_url}/chat/completions` with the key as a bearer
//! token. 429 and 5xx answers are retried after each [`OpenAiClient`]
//! backoff delay; any other 4xx fails at once, except for two once-only
//! downgrades on a 400 that does not use up a retry: a 400 about
//! `response_format` on a `json_schema` request is resent as `json_object`
//! (providers without structured outputs; the caller validates the JSON),
//! and a 400 naming `max_completion_tokens` is resent with the cap as
//! `max_tokens` (providers that predate OpenAI's rename). No error or log
//! line carries the key.

use std::error::Error;
use std::iter;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time;
use tracing::{info, warn};

use super::client::{
    AiClient, AiError, ChatRequest, ChatResponse, ContentPart, Detail, ResponseFormat, Role, Usage,
};
use crate::svc::ai_settings::AiConfig;

/// How long one request may take, from connecting to the last body byte.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The waits before the retries of a 429 or 5xx; one retry per entry.
const BACKOFF: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(4)];
/// The longest error body kept, in characters.
const SNIPPET_CHARS: usize = 500;
/// What replaces the key wherever a provider echoes it.
const MASK: &str = "***";

/// Calls an OpenAI-compatible provider. Stateless apart from the connection
/// pool: every call takes the current [`AiConfig`], so saved settings apply
/// to the next call.
pub struct OpenAiClient {
    http: Client,
    timeout: Duration,
    backoff: Vec<Duration>,
}

impl OpenAiClient {
    /// A client with a 60 s request timeout, a 10 s connect timeout and
    /// retries after 1 s and 4 s.
    ///
    /// # Panics
    ///
    /// If the TLS backend cannot initialise, as `reqwest::Client::new` would;
    /// this runs once, at startup.
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("the HTTP client's TLS backend failed to initialise");
        Self {
            http,
            timeout: REQUEST_TIMEOUT,
            backoff: BACKOFF.to_vec(),
        }
    }

    /// This client with `timeout` as the whole-request timeout.
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    /// This client waiting `backoff[n]` before retry `n + 1` of a 429 or
    /// 5xx; `backoff.len()` is the number of retries.
    pub fn with_backoff(self, backoff: Vec<Duration>) -> Self {
        Self { backoff, ..self }
    }

    /// One HTTP exchange; any status is a [`Reply`], only transport
    /// failures are errors.
    async fn post(&self, url: &str, key: &str, body: &WireRequest<'_>) -> Result<Reply, AiError> {
        let response = self
            .http
            .post(url)
            .bearer_auth(key)
            // Covers connecting through the last body byte, so a provider
            // that trickles its body out still times out.
            .timeout(self.timeout)
            .json(body)
            .send()
            .await
            .map_err(|err| transport_error(err, key))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| transport_error(err, key))?;
        Ok(Reply { status, text })
    }
}

impl Default for OpenAiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AiClient for OpenAiClient {
    async fn chat(&self, config: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
        let key = config.api_key.as_str();
        let mut wire = WireRequest::from(&request);
        let mut retries = self.backoff.iter();
        let started = Instant::now();
        loop {
            let reply = self.post(&url, key, &wire).await?;
            if reply.status.is_success() {
                let response = parse(&reply.text, &request.model)?;
                let usage = response.usage;
                info!(
                    model = %response.model,
                    status = reply.status.as_u16(),
                    latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    prompt_tokens = usage.map(|u| u.prompt_tokens),
                    completion_tokens = usage.map(|u| u.completion_tokens),
                    "AI call answered"
                );
                return Ok(response);
            }

            let snippet = snippet(&reply.text, key);
            warn!(
                model = %request.model,
                status = reply.status.as_u16(),
                body = %snippet,
                "AI provider refused the call"
            );
            if wire.downgrade_for(&reply) {
                continue;
            }
            if is_retryable(reply.status)
                && let Some(delay) = retries.next()
            {
                time::sleep(*delay).await;
                continue;
            }
            return Err(AiError::Status {
                status: reply.status.as_u16(),
                snippet,
            });
        }
    }
}

/// A provider's answer, whatever its status.
struct Reply {
    status: StatusCode,
    text: String,
}

/// Rate limits and server errors are worth another try; other 4xx are not.
fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// A reqwest failure as an [`AiError`]: a timeout, or a transport error
/// whose message (with its causes) omits the URL, which may carry
/// credentials, and the key.
fn transport_error(err: reqwest::Error, key: &str) -> AiError {
    if err.is_timeout() {
        return AiError::Timeout;
    }
    let err = err.without_url();
    let message = iter::successors(Some(&err as &dyn Error), |e| (*e).source())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ");
    AiError::Transport(mask(&message, key))
}

/// `text` with every occurrence of `key` replaced by [`MASK`].
fn mask(text: &str, key: &str) -> String {
    if key.is_empty() {
        text.to_owned()
    } else {
        text.replace(key, MASK)
    }
}

/// An error body fit for a message or a log: key masked, then cut to
/// [`SNIPPET_CHARS`] (masking first, so a cut cannot leave part of the key).
fn snippet(body: &str, key: &str) -> String {
    mask(body, key).chars().take(SNIPPET_CHARS).collect()
}

/// A 2xx body as a [`ChatResponse`]; `requested` stands in for a provider
/// that does not name the model.
fn parse(body: &str, requested: &str) -> Result<ChatResponse, AiError> {
    let wire: WireResponse =
        serde_json::from_str(body).map_err(|err| AiError::Malformed(err.to_string()))?;
    let content = wire
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .ok_or_else(|| AiError::Malformed("the answer has no message content".to_owned()))?;
    Ok(ChatResponse {
        content: content.into_text(),
        usage: wire.usage.and_then(WireUsage::complete),
        model: wire.model.unwrap_or_else(|| requested.to_owned()),
    })
}

/// The request body, borrowing from the [`ChatRequest`].
#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<WireFormat<'a>>,
    /// OpenAI's current name for the cap: its reasoning models (the default
    /// `gpt-5-mini` among them) refuse the older `max_tokens` with a 400.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    /// The cap under its older name, sent only after a provider refused
    /// `max_completion_tokens`.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

impl<'a> From<&'a ChatRequest> for WireRequest<'a> {
    fn from(request: &'a ChatRequest) -> Self {
        Self {
            model: &request.model,
            messages: request
                .messages
                .iter()
                .map(|message| WireMessage {
                    role: message.role.into(),
                    content: message.content.iter().map(WirePart::from).collect(),
                })
                .collect(),
            response_format: request.response_format.as_ref().map(WireFormat::from),
            max_completion_tokens: request.max_tokens,
            max_tokens: None,
        }
    }
}

impl WireRequest<'_> {
    /// Rewrites this request for a provider whose 400 `reply` refused a
    /// field it can do without; `false` when there is nothing to downgrade.
    /// Each downgrade removes what triggers it (`json_schema`, or the
    /// `max_completion_tokens` cap), so each happens at most once per call.
    fn downgrade_for(&mut self, reply: &Reply) -> bool {
        if reply.status != StatusCode::BAD_REQUEST {
            return false;
        }
        let body = reply.text.to_ascii_lowercase();
        // A schema the provider understood but found invalid is our bug:
        // surface it rather than silently dropping to `json_object`.
        if matches!(self.response_format, Some(WireFormat::JsonSchema { .. }))
            && body.contains("response_format")
            && !body.contains("invalid schema")
        {
            self.response_format = Some(WireFormat::JsonObject);
            return true;
        }
        if self.max_completion_tokens.is_some() && body.contains("max_completion_tokens") {
            self.max_tokens = self.max_completion_tokens.take();
            return true;
        }
        false
    }
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: WireRole,
    content: Vec<WirePart<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum WireRole {
    System,
    User,
}

impl From<Role> for WireRole {
    fn from(role: Role) -> Self {
        match role {
            Role::System => Self::System,
            Role::User => Self::User,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WirePart<'a> {
    Text { text: &'a str },
    ImageUrl { image_url: WireImage<'a> },
}

impl<'a> From<&'a ContentPart> for WirePart<'a> {
    fn from(part: &'a ContentPart) -> Self {
        match part {
            ContentPart::Text(text) => Self::Text { text },
            ContentPart::ImageUrl { url, detail } => Self::ImageUrl {
                image_url: WireImage {
                    url,
                    detail: (*detail).into(),
                },
            },
        }
    }
}

#[derive(Serialize)]
struct WireImage<'a> {
    url: &'a str,
    detail: WireDetail,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum WireDetail {
    Auto,
    Low,
    High,
}

impl From<Detail> for WireDetail {
    fn from(detail: Detail) -> Self {
        match detail {
            Detail::Auto => Self::Auto,
            Detail::Low => Self::Low,
            Detail::High => Self::High,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireFormat<'a> {
    JsonSchema { json_schema: WireSchema<'a> },
    JsonObject,
}

impl<'a> From<&'a ResponseFormat> for WireFormat<'a> {
    fn from(format: &'a ResponseFormat) -> Self {
        match format {
            ResponseFormat::JsonSchema { name, schema } => Self::JsonSchema {
                json_schema: WireSchema {
                    name,
                    schema,
                    strict: true,
                },
            },
            ResponseFormat::JsonObject => Self::JsonObject,
        }
    }
}

#[derive(Serialize)]
struct WireSchema<'a> {
    name: &'a str,
    schema: &'a Value,
    strict: bool,
}

/// The parts of a completion the client reads; everything else is ignored.
#[derive(Deserialize)]
struct WireResponse {
    model: Option<String>,
    #[serde(default)]
    choices: Vec<WireChoice>,
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireAnswer,
}

#[derive(Deserialize)]
struct WireAnswer {
    content: Option<WireContent>,
}

/// A message's content: a string, or (some providers) an array of parts.
#[derive(Deserialize)]
#[serde(untagged)]
enum WireContent {
    Text(String),
    Parts(Vec<WireAnswerPart>),
}

impl WireContent {
    /// The text, with an array's `text` parts joined in order.
    fn into_text(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Parts(parts) => parts.into_iter().filter_map(|part| part.text).collect(),
        }
    }
}

#[derive(Deserialize)]
struct WireAnswerPart {
    text: Option<String>,
}

/// Token counts; compatible providers sometimes omit one.
#[derive(Deserialize)]
struct WireUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

impl WireUsage {
    /// Both counts, or `None` when either is missing.
    fn complete(self) -> Option<Usage> {
        Some(Usage {
            prompt_tokens: self.prompt_tokens?,
            completion_tokens: self.completion_tokens?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_mask_the_key_before_truncating() {
        let key = "sk-secret";
        let body = format!("{}{key}", "x".repeat(495));
        let cut = snippet(&body, key);
        assert_eq!(cut, format!("{}***", "x".repeat(495)));
        assert!(!snippet(&format!("{}{key}", "x".repeat(498)), key).contains("sk-"));
    }

    #[test]
    fn only_429_and_5xx_are_retryable() {
        for status in [429, 500, 502, 503] {
            assert!(is_retryable(StatusCode::from_u16(status).unwrap()));
        }
        for status in [400, 401, 403, 404, 422] {
            assert!(!is_retryable(StatusCode::from_u16(status).unwrap()));
        }
    }

    #[test]
    fn a_response_without_a_model_reports_the_requested_one() {
        let response = parse(r#"{"choices":[{"message":{"content":"hi"}}]}"#, "m").unwrap();
        assert_eq!(response.model, "m");
        assert_eq!(response.usage, None);
    }
}
