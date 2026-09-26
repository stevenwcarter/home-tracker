//! The authorization seam: who is making each request.
//!
//! A router-level middleware puts an [`Actor`] into every request's
//! extensions, and handlers read `Extension<Actor>` rather than deciding for
//! themselves. v1 has no authentication, so everyone is [`Actor::Anonymous`];
//! when auth arrives, only this middleware changes.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::graphql::context::Actor;

/// Attaches the request's [`Actor`] (always [`Actor::Anonymous`] in v1).
pub async fn attach_actor(mut request: Request, next: Next) -> Response {
    request.extensions_mut().insert(Actor::Anonymous);
    next.run(request).await
}
