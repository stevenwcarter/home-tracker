use axum_test::TestServer;
use home_tracker::db::TestDb;
use home_tracker::routes::app;
use home_tracker::svc::fixtures::seed_sample;
use serde_json::{Value, json};

#[tokio::test]
async fn summary_query_reports_the_sample_statistics() {
    let db = TestDb::new();
    {
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
    }
    let server = TestServer::new(app(db.pool.clone()));

    let response = server
        .post("/graphql")
        .json(&json!({
            "query": "{ summary { totalValueCents currency totalItems totalLocations totalTags } }"
        }))
        .await;

    response.assert_status_ok();
    let body: Value = response.json();
    assert_eq!(
        body,
        json!({
            "data": {
                "summary": {
                    "totalValueCents": 67_297,
                    "currency": "USD",
                    "totalItems": 4,
                    "totalLocations": 4,
                    "totalTags": 2
                }
            }
        })
    );
}

#[tokio::test]
async fn get_is_not_allowed() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone()));

    let response = server.get("/graphql?query={summary{currency}}").await;

    response.assert_status(axum::http::StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn unknown_field_is_a_graphql_error_not_a_500() {
    let db = TestDb::new();
    let server = TestServer::new(app(db.pool.clone()));

    let response = server
        .post("/graphql")
        .json(&json!({ "query": "{ nope }" }))
        .await;

    response.assert_status_bad_request();
    let body: Value = response.json();
    assert!(body["errors"].is_array());
}
