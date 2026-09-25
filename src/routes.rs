//! Top-level router: GraphQL, static assets, SPA fallback, compression.

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;
use tower_http::compression::CompressionLayer;

use crate::api::graphql::graphql_routes;
use crate::db::SqlitePool;
use crate::graphql::schema::create_schema;

/// The Vite build output. `site/build` must exist at compile time (see `just site-placeholder`).
#[derive(RustEmbed, Clone)]
#[folder = "site/build/"]
struct Assets;

fn serve_asset(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

async fn static_handler(uri: Uri) -> Response {
    serve_asset(uri.path().trim_start_matches('/'))
}

/// Every non-asset, non-API path serves the SPA shell so client routing can take over.
async fn index_handler() -> Response {
    serve_asset("index.html")
}

async fn immutable_cache(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
}

pub fn app(pool: SqlitePool) -> Router {
    let schema = Arc::new(create_schema());
    Router::new()
        .route("/assets/{*path}", get(static_handler))
        .layer(middleware::from_fn(immutable_cache))
        .merge(graphql_routes(pool, schema))
        .route("/", get(index_handler))
        .fallback(get(index_handler))
        .layer(CompressionLayer::new())
}
