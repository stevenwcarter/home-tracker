//! The seam between the importer and the GraphQL surface: a Homebox backup
//! imported through `import_backup` must read back through `/graphql` with the
//! values the backup holds, so a change on either side that breaks the other
//! fails here even when both sides' own suites stay green.

// Each test crate uses a different subset of the shared fixture builder.
#[allow(dead_code)]
mod support;

use axum_test::TestServer;
use home_tracker::db::TestDb;
use home_tracker::import::run::import_backup;
use home_tracker::import::source::DirSource;
use home_tracker::routes::app;
use serde_json::{Value, json};
use support::MiniBackup;

#[tokio::test]
async fn an_imported_backup_reads_back_through_graphql() {
    let backup = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let ids = MiniBackup::write_dir(backup.path());
    let db = TestDb::new();
    import_backup(
        &mut db.pool.get().unwrap(),
        &mut DirSource::new(backup.path()),
        data.path(),
    )
    .unwrap();
    let server = TestServer::new(app(db.pool.clone()));

    let response = server
        .post("/graphql")
        .json(&json!({
            "query": "query($id: ID!) {
                summary { totalValueCents currency totalItems totalLocations totalTags }
                locations { name parentId }
                entity(id: $id) {
                    assetId purchasePriceCents purchaseDate insured
                    entityType { name } parent { name } tags { name }
                    fields { name textValue } primaryPhoto { url thumbnailUrl }
                }
            }",
            "variables": { "id": ids.router },
        }))
        .await;
    response.assert_status_ok();
    let body: Value = response.json();
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    let data = &body["data"];

    // Router 1099.99 × 1 + Cable 4.50 × 3 = 1113.49; Garage and Tote 1 (a
    // location type) are locations, Router and Cable items; IOT and General tags.
    assert_eq!(
        data["summary"],
        json!({
            "totalValueCents": 111_349,
            "currency": "USD",
            "totalItems": 2,
            "totalLocations": 2,
            "totalTags": 2,
        })
    );
    // A flat list, not a tree: the client nests it itself via `parentId`.
    let locations = data["locations"].as_array().unwrap();
    let by_name = |name: &str| locations.iter().find(|l| l["name"] == name).unwrap();
    assert_eq!(by_name("Garage")["parentId"], Value::Null);
    assert_eq!(by_name("Tote 1")["parentId"], ids.garage);

    let router = &data["entity"];
    assert_eq!(router["assetId"], "000-007");
    assert_eq!(router["purchasePriceCents"], 109_999);
    assert_eq!(router["purchaseDate"], "2021-10-23");
    assert_eq!(router["insured"], true);
    assert_eq!(router["entityType"]["name"], "Item");
    assert_eq!(router["parent"]["name"], "Garage");
    assert_eq!(router["tags"], json!([{ "name": "IOT" }]));
    assert_eq!(
        router["fields"],
        json!([{ "name": "Model", "textValue": "AX1800" }])
    );
    let photo = &router["primaryPhoto"];
    let version = &ids.jpeg_sha256[..12];
    assert_eq!(
        photo["url"],
        format!("/attachments/{}?v={version}", ids.photo)
    );
    assert_eq!(
        photo["thumbnailUrl"],
        format!("/attachments/{}/thumb/500?v={version}", ids.photo)
    );
}
