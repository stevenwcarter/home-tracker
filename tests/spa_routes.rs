use axum_test::TestServer;
use home_tracker::db::TestDb;
use home_tracker::routes::app;

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
}

#[tokio::test]
async fn missing_assets_are_404_not_the_shell() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone()));
    server
        .get("/assets/does-not-exist.js")
        .await
        .assert_status_not_found();
}
