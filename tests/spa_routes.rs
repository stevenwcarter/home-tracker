#[cfg(debug_assertions)]
use std::fs;
#[cfg(debug_assertions)]
use std::path::PathBuf;

use axum_test::{TestResponse, TestServer};
use home_tracker::db::TestDb;
use home_tracker::routes::app;
#[cfg(debug_assertions)]
use uuid::Uuid;

// `site/build/index.html` must exist at compile time; `just site-placeholder` creates it.

#[tokio::test]
async fn root_serves_the_spa_shell_as_html() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone()));
    let response = server.get("/").await;
    response.assert_status_ok();
    assert!(
        response
            .header("content-type")
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_not_immutably_cached(&response);
}

#[tokio::test]
async fn deep_links_fall_back_to_the_spa_shell() {
    // Review focus 4: a reload on /locations/abc must boot the SPA, not 404.
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone()));
    let shell = server.get("/").await;
    let deep = server.get("/locations/abc").await;
    deep.assert_status_ok();
    assert!(
        deep.header("content-type")
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_eq!(deep.text(), shell.text());
    assert_not_immutably_cached(&deep);
}

#[tokio::test]
async fn missing_assets_are_404_not_the_shell() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone()));
    let response = server.get("/assets/does-not-exist.js").await;
    response.assert_status_not_found();
    // Review focus (minor): a 404 must not be cached immutably, or a cache sitting in
    // front of the app would keep serving it for a year after the asset is deployed.
    assert_not_immutably_cached(&response);
}

/// `rust-embed` without the `debug-embed` feature (not enabled in this crate's `Cargo.toml`)
/// reads its embedded folder straight from disk, on every call, whenever the binary is built
/// with `cfg(debug_assertions)` — i.e. an ordinary `cargo test` run — rather than baking file
/// bytes in at compile time. That is what lets this test drop a file into
/// `site/build/assets/` and have `GET /assets/<name>` serve it immediately. Release builds
/// embed every file's bytes at compile time instead, so a file written after the binary is
/// built would never be found; this test (and the behaviour it pins) is debug-only.
#[cfg(debug_assertions)]
#[tokio::test]
async fn a_real_asset_is_served_with_the_immutable_cache_header() {
    let name = format!("{}.js", Uuid::now_v7());
    let _asset = TempAsset::create(&name, "// test asset");

    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone()));
    let response = server.get(&format!("/assets/{name}")).await;

    response.assert_status_ok();
    assert_eq!(
        response.header("cache-control").to_str().unwrap(),
        "public, max-age=31536000, immutable"
    );
    let content_type = response.header("content-type").to_str().unwrap().to_owned();
    assert!(
        content_type.starts_with("text/javascript")
            || content_type.starts_with("application/javascript"),
        "unexpected content-type: {content_type}"
    );
}

/// `cache-control` must never contain `immutable` on a response that is not a real,
/// content-addressed static asset — otherwise a cache would keep serving a stale SPA shell
/// (or a stale 404) long after a new deploy replaces it.
fn assert_not_immutably_cached(response: &TestResponse) {
    let cache_control = response
        .maybe_header("cache-control")
        .map(|value| value.to_str().unwrap_or_default().to_owned())
        .unwrap_or_default();
    assert!(
        !cache_control.contains("immutable"),
        "got {cache_control:?}"
    );
}

/// Creates a file under `site/build/assets/` for the debug-only embed test above and removes
/// it on drop, so a failing assertion still cleans up.
#[cfg(debug_assertions)]
struct TempAsset {
    path: PathBuf,
}

#[cfg(debug_assertions)]
impl TempAsset {
    fn create(name: &str, contents: &str) -> Self {
        let dir = PathBuf::from("site/build/assets");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        Self { path }
    }
}

#[cfg(debug_assertions)]
impl Drop for TempAsset {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
