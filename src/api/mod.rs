//! Plain HTTP handlers (everything that is not GraphQL).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub mod graphql;

/// `anyhow::Error` → 500 with the message. Handlers return `Result<_, AppError>` and use `?`.
pub struct AppError(pub anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
