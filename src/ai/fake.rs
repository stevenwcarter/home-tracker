//! A scripted [`AiClient`] for tests: it answers from a queue and records
//! every request. Public rather than test-gated so integration tests (and
//! the ingest runner's tests) can drive the GraphQL flow with it.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;

use super::client::{AiClient, AiError, ChatRequest, ChatResponse};
use crate::svc::ai_settings::AiConfig;

/// Answers each call with the next queued result, in push order.
#[derive(Default)]
pub struct FakeAiClient {
    answers: Mutex<VecDeque<Result<String, AiError>>>,
    requests: Mutex<Vec<ChatRequest>>,
}

impl FakeAiClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues the content (or error) of a future call's answer.
    pub fn push(&self, answer: Result<String, AiError>) {
        lock(&self.answers).push_back(answer);
    }

    /// Every request received so far, in call order.
    pub fn requests(&self) -> Vec<ChatRequest> {
        lock(&self.requests).clone()
    }
}

/// A poisoned lock only means another test thread panicked; the queue is
/// still consistent, so carry on with it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[async_trait]
impl AiClient for FakeAiClient {
    async fn chat(&self, _: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError> {
        let model = request.model.clone();
        lock(&self.requests).push(request);
        let content = lock(&self.answers)
            .pop_front()
            .unwrap_or_else(|| Err(AiError::Malformed("fake queue empty".to_owned())))?;
        Ok(ChatResponse {
            content,
            usage: None,
            model,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::client::{ContentPart, Message, Role};

    fn request(model: &str) -> ChatRequest {
        ChatRequest {
            model: model.to_owned(),
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentPart::Text("hi".to_owned())],
            }],
            response_format: None,
            max_tokens: None,
        }
    }

    #[tokio::test]
    async fn answers_in_push_order_then_reports_an_empty_queue() {
        let config = AiConfig {
            base_url: "http://unused".to_owned(),
            api_key: "k".to_owned(),
            vision_model: "v".to_owned(),
            synthesis_model: "s".to_owned(),
            extra_instructions: None,
        };
        let fake = FakeAiClient::new();
        fake.push(Ok("first".to_owned()));
        fake.push(Err(AiError::Timeout));

        let first = fake.chat(&config, request("a")).await.unwrap();
        assert_eq!(
            (first.content.as_str(), first.model.as_str()),
            ("first", "a")
        );
        assert_eq!(
            fake.chat(&config, request("b")).await,
            Err(AiError::Timeout)
        );
        assert_eq!(
            fake.chat(&config, request("c")).await,
            Err(AiError::Malformed("fake queue empty".to_owned()))
        );
        assert_eq!(fake.requests(), [request("a"), request("b"), request("c")]);
    }
}
