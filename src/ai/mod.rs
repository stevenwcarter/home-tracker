//! Talking to an OpenAI-compatible model: the client seam and the
//! process-wide AI state the router shares with every request.

pub mod client;
pub mod env;

use std::sync::Arc;

use async_trait::async_trait;

use self::client::{AiClient, AiError, ChatRequest, ChatResponse};
use self::env::AiEnv;
use crate::svc::ai_settings::AiConfig;

/// What a `Debug` impl prints in place of the API key.
pub(crate) const REDACTED: &str = "<redacted>";

/// What every GraphQL request needs to reach the model: the environment
/// overrides read at startup and the client that makes the calls.
pub struct AiState {
    pub env: AiEnv,
    pub client: Arc<dyn AiClient>,
}

impl AiState {
    /// No environment overrides and a client that refuses every call, for
    /// tests and for any router built without a real client.
    pub fn disabled() -> Self {
        Self {
            env: AiEnv::none(),
            client: Arc::new(DisabledClient),
        }
    }
}

/// A client that never calls out: every request is [`AiError::NotConfigured`].
struct DisabledClient;

#[async_trait]
impl AiClient for DisabledClient {
    async fn chat(&self, _: &AiConfig, _: ChatRequest) -> Result<ChatResponse, AiError> {
        Err(AiError::NotConfigured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_disabled_client_refuses_every_call() {
        let state = AiState::disabled();
        assert_eq!(state.env, AiEnv::none());
        let config = AiConfig {
            base_url: "https://api.openai.com/v1".to_owned(),
            api_key: "k".to_owned(),
            vision_model: "m".to_owned(),
            synthesis_model: "m".to_owned(),
            extra_instructions: None,
        };
        let request = ChatRequest {
            model: "m".to_owned(),
            messages: Vec::new(),
            response_format: None,
            max_tokens: None,
        };
        assert_eq!(
            state.client.chat(&config, request).await,
            Err(AiError::NotConfigured)
        );
    }
}
