//! Top-level router: GraphQL, attachments, uploads, static assets, SPA
//! fallback, compression (everything but attachments and uploads), and the
//! actor seam around all of it.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use rust_embed::RustEmbed;
use tower_http::compression::CompressionLayer;

use crate::api::IMMUTABLE_CACHE;
use crate::api::actor::attach_actor;
use crate::api::attachments::attachment_routes;
use crate::api::graphql::graphql_routes;
use crate::api::upload::upload_routes;
use crate::db::SqlitePool;
use crate::graphql::context::Actor;
use crate::graphql::schema::create_schema;
use crate::svc::thumbnail_service::ThumbnailService;

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
    // Only a successful response is a real, content-addressed asset. Caching a 404
    // immutably would make a missing (or not-yet-deployed) asset stay missing for a
    // year in front of any cache.
    if response.status().is_success() {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, IMMUTABLE_CACHE);
    }
    response
}

/// The whole application; originals and thumbnails are read from `data_dir`.
pub fn app(pool: SqlitePool, data_dir: PathBuf) -> Router {
    let thumbnails = ThumbnailService::new(pool.clone(), data_dir);
    app_with_thumbnails(pool, thumbnails)
}

/// [`app`] around a caller-supplied thumbnail service, so tests can observe it.
pub fn app_with_thumbnails(pool: SqlitePool, thumbnails: Arc<ThumbnailService>) -> Router {
    // Outermost, so every route (and the fallback) sees the actor.
    routes(pool, thumbnails).layer(middleware::from_fn(attach_actor))
}

/// Test support: [`app`] with every request made by `actor` instead of the
/// one [`attach_actor`] would attach, for tests of what a read-only user may
/// do. Production uses [`app`].
pub fn app_with_actor(pool: SqlitePool, data_dir: PathBuf, actor: Actor) -> Router {
    let thumbnails = ThumbnailService::new(pool.clone(), data_dir);
    routes(pool, thumbnails).layer(Extension(actor))
}

/// Every route, without the actor layer.
fn routes(pool: SqlitePool, thumbnails: Arc<ThumbnailService>) -> Router {
    let schema = Arc::new(create_schema());
    // The thumbnail service's data dir is the one the attachment routes read,
    // so a GraphQL delete removes originals from the same place.
    let data_dir = Arc::from(thumbnails.data_dir());
    let compressed = Router::new()
        .route("/assets/{*path}", get(static_handler))
        // Load-bearing position: `Router::layer` only wraps routes already added, so
        // this must stay after `/assets` and before `/`/the fallback, or the SPA shell
        // (and a missing-asset 404) would be cached immutably too. Pinned by
        // tests/spa_routes.rs.
        .layer(middleware::from_fn(immutable_cache))
        .merge(graphql_routes(pool.clone(), schema, data_dir))
        .route("/", get(index_handler))
        .fallback(get(index_handler))
        .layer(CompressionLayer::new());
    // Merged outside the compression layer: originals are served byte-for-byte
    // under their sha256 ETag, and photos gain nothing from re-encoding.
    // Pinned by tests/attachments.rs. Uploads answer small JSON bodies.
    let upload_dir = Arc::new(thumbnails.data_dir().to_path_buf());
    Router::new()
        .merge(upload_routes(pool.clone(), upload_dir))
        .merge(attachment_routes(pool, thumbnails))
        .merge(compressed)
}
