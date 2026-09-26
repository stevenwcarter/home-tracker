use std::path::Path;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Extension, Router};
use juniper_axum::extract::JuniperRequest;
use juniper_axum::graphiql;
use juniper_axum::response::JuniperResponse;

use crate::db::SqlitePool;
use crate::graphql::context::{Actor, GraphQLContext};
use crate::graphql::schema::Schema;

/// `/graphql` (POST only; there is no auth in v1, so a GET-triggered mutation
/// would be a LAN CSRF path) plus GraphiQL at `/graphiql` in debug builds.
/// Mutations that delete attachments remove originals from `data_dir`.
pub fn graphql_routes(pool: SqlitePool, schema: Arc<Schema>, data_dir: Arc<Path>) -> Router {
    let router = Router::new().route("/graphql", post(handle));
    let router = if cfg!(debug_assertions) {
        router.route("/graphiql", get(graphiql("/graphql", None)))
    } else {
        router
    };
    router
        .layer(Extension(pool))
        .layer(Extension(schema))
        .layer(Extension(data_dir))
}

async fn handle(
    Extension(actor): Extension<Actor>,
    Extension(schema): Extension<Arc<Schema>>,
    Extension(pool): Extension<SqlitePool>,
    Extension(data_dir): Extension<Arc<Path>>,
    JuniperRequest(request): JuniperRequest,
) -> JuniperResponse {
    // A fresh context per request, for the actor the router attached
    // (see `api::actor`).
    let context = GraphQLContext::new(pool, actor, data_dir);
    JuniperResponse(request.execute(&*schema, &context).await)
}
