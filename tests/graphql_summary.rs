use axum_test::TestServer;
use home_tracker::db::TestDb;
use home_tracker::routes::app;
use serde_json::{Value, json};

#[tokio::test]
async fn summary_query_returns_the_placeholder_numbers_and_currency() {
    let db = TestDb::new();
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
                    "totalValueCents": 1_234_567,
                    "currency": "USD",
                    "totalItems": 42,
                    "totalLocations": 7,
                    "totalTags": 5
                }
            }
        })
    );
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
