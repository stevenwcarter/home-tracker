//! The model-call seam: one chat completion in, one answer out.
//!
//! Callers build a [`ChatRequest`] and hand it to an [`AiClient`] with the
//! resolved [`AiConfig`]; the implementation owns the wire format, retries
//! and timeouts. Nothing here carries the API key except the config, and no
//! [`AiError`] message ever includes it.

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

use crate::svc::ai_settings::AiConfig;

/// One chat completion request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    /// `None` lets the model answer free text.
    pub response_format: Option<ResponseFormat>,
    pub max_tokens: Option<u32>,
}

/// One turn of the conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

/// Who a [`Message`] is from. Only the two roles a request sends exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
}

/// A piece of a message: text, or an image by URL (a `data:` URL for photos).
#[derive(Debug, Clone, PartialEq)]
pub enum ContentPart {
    Text(String),
    ImageUrl { url: String, detail: Detail },
}

/// How closely the model looks at an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Auto,
    Low,
    High,
}

/// The shape the answer must take.
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseFormat {
    /// Strict structured output against `schema`.
    JsonSchema { name: String, schema: Value },
    /// Any JSON object; the caller validates it.
    JsonObject,
}

/// The model's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatResponse {
    pub content: String,
    pub usage: Option<Usage>,
    /// The model that answered, as the provider reports it.
    pub model: String,
}

/// Token counts the provider reports for one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Why a model call failed. `Display` never includes the API key.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AiError {
    #[error("AI is not configured: add an API key first")]
    NotConfigured,
    #[error("could not reach the AI provider: {0}")]
    Transport(String),
    #[error("the AI provider did not answer in time")]
    Timeout,
    /// `snippet` is the response body, truncated, with the key masked.
    #[error("the AI provider answered HTTP {status}: {snippet}")]
    Status { status: u16, snippet: String },
    #[error("the AI provider's answer was malformed: {0}")]
    Malformed(String),
}

/// Sends chat completions. Object-safe so the router can hold any client.
#[async_trait]
pub trait AiClient: Send + Sync {
    async fn chat(&self, config: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError>;
}
