use std::sync::Arc;

use axum::routing::{MethodFilter, get, on};
use axum::{Extension, Router};
use juniper_axum::extract::JuniperRequest;
use juniper_axum::graphiql;
use juniper_axum::response::JuniperResponse;

use crate::db::SqlitePool;
use crate::graphql::context::{Actor, GraphQLContext};
use crate::graphql::schema::Schema;

/// `/graphql` (GET and POST) plus GraphiQL at `/graphiql` in debug builds.
pub fn graphql_routes(pool: SqlitePool, schema: Arc<Schema>) -> Router {
    let router = Router::new().route(
        "/graphql",
        on(MethodFilter::GET.or(MethodFilter::POST), handle),
    );
    let router = if cfg!(debug_assertions) {
        router.route("/graphiql", get(graphiql("/graphql", None)))
    } else {
        router
    };
    router.layer(Extension(pool)).layer(Extension(schema))
}

async fn handle(
    Extension(schema): Extension<Arc<Schema>>,
    Extension(pool): Extension<SqlitePool>,
    JuniperRequest(request): JuniperRequest,
) -> JuniperResponse {
    // A fresh context per request: auth will fill `actor` from the request here.
    let context = GraphQLContext::new(pool, Actor::Anonymous);
    JuniperResponse(request.execute(&*schema, &context).await)
}
