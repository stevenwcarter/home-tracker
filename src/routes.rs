//! Top-level router: GraphQL, attachments, uploads, ingest photos, static
//! assets, SPA fallback, compression (everything but attachments, uploads and
//! the ingest routes), and the actor seam around all of it.

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

use crate::ai::AiState;
use crate::ai::env::AiEnv;
use crate::ai::openai::OpenAiClient;
use crate::api::IMMUTABLE_CACHE;
use crate::api::actor::attach_actor;
use crate::api::attachments::attachment_routes;
use crate::api::graphql::graphql_routes;
use crate::api::ingest::ingest_routes;
use crate::api::upload::upload_routes;
use crate::db::SqlitePool;
use crate::graphql::context::Actor;
use crate::graphql::schema::create_schema;
use crate::ingest::events::IngestEvents;
use crate::ingest::runner::IngestRunner;
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

/// The whole application; originals and thumbnails are read from `data_dir`,
/// and the AI environment overrides are read here, once.
pub fn app(pool: SqlitePool, data_dir: PathBuf) -> Router {
    app_with_runner(pool, data_dir).0
}

/// [`app`] and the ingest runner its routes share, so startup can resume
/// interrupted batches and schedule cleanup on that same runner.
pub fn app_with_runner(pool: SqlitePool, data_dir: PathBuf) -> (Router, Arc<IngestRunner>) {
    let env = AiEnv::from_env();
    tracing::info!(overrides = ?env.overridden(), "AI environment");
    let ai = Arc::new(AiState {
        env,
        client: Arc::new(OpenAiClient::new()),
    });
    let thumbnails = ThumbnailService::new(pool.clone(), data_dir);
    let (router, runner) = routes(pool, thumbnails, ai);
    (with_actor_seam(router), runner)
}

/// [`app`] around a caller-supplied thumbnail service, so tests can observe
/// it. AI is disabled.
pub fn app_with_thumbnails(pool: SqlitePool, thumbnails: Arc<ThumbnailService>) -> Router {
    with_actor_seam(routes(pool, thumbnails, Arc::new(AiState::disabled())).0)
}

/// Test support: [`app`] with a caller-supplied AI state, so tests set the
/// environment overrides and the model client without touching the process
/// environment.
pub fn app_with_ai(pool: SqlitePool, data_dir: PathBuf, ai: Arc<AiState>) -> Router {
    let thumbnails = ThumbnailService::new(pool.clone(), data_dir);
    with_actor_seam(routes(pool, thumbnails, ai).0)
}

/// Test support: [`app`] with every request made by `actor` instead of the
/// one [`attach_actor`] would attach, for tests of what a read-only user may
/// do. AI is disabled. Production uses [`app`].
pub fn app_with_actor(pool: SqlitePool, data_dir: PathBuf, actor: Actor) -> Router {
    let thumbnails = ThumbnailService::new(pool.clone(), data_dir);
    routes(pool, thumbnails, Arc::new(AiState::disabled()))
        .0
        .layer(Extension(actor))
}

/// `router` with [`attach_actor`] outermost, so every route (and the
/// fallback) sees the actor.
fn with_actor_seam(router: Router) -> Router {
    router.layer(middleware::from_fn(attach_actor))
}

/// Every route, without the actor layer, and the one ingest runner they
/// share, built over the same thumbnail service and AI state.
fn routes(
    pool: SqlitePool,
    thumbnails: Arc<ThumbnailService>,
    ai: Arc<AiState>,
) -> (Router, Arc<IngestRunner>) {
    let runner = IngestRunner::new(
        pool.clone(),
        Arc::clone(&thumbnails),
        Arc::clone(&ai),
        IngestEvents::new(),
    );
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
        .merge(graphql_routes(
            pool.clone(),
            schema,
            data_dir,
            ai,
            Arc::clone(&runner),
        ))
        .route("/", get(index_handler))
        .fallback(get(index_handler))
        .layer(CompressionLayer::new());
    // Merged outside the compression layer: originals are served byte-for-byte
    // under their sha256 ETag, and photos gain nothing from re-encoding.
    // Pinned by tests/attachments.rs. Uploads answer small JSON bodies. The
    // ingest routes are both, a staging upload and staged originals, plus
    // the progress stream. (tower-http's default predicate would skip
    // `text/event-stream` inside the compressed router too, so
    // tests/ingest_events.rs pins the uncompressed response, not this mount.)
    let upload_dir = Arc::new(thumbnails.data_dir().to_path_buf());
    let router = Router::new()
        .merge(upload_routes(pool.clone(), Arc::clone(&upload_dir)))
        .merge(ingest_routes(
            pool.clone(),
            upload_dir,
            Arc::clone(&thumbnails),
            Arc::clone(runner.events()),
        ))
        .merge(attachment_routes(pool, thumbnails))
        .merge(compressed);
    (router, runner)
}
