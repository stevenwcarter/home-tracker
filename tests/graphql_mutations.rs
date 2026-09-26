use std::fs;

use axum_test::TestServer;
use diesel::prelude::*;
use home_tracker::db::{ITEM_TYPE_ID, LOCATION_TYPE_ID, TestDb};
use home_tracker::kinds::AttachmentKind;
use home_tracker::routes::app;
use home_tracker::schema::attachments;
use home_tracker::svc::attachment::original_path;
use home_tracker::svc::fixtures::{self, SampleIds, seed_sample};
use serde_json::{Value, json};
use tempfile::TempDir;

/// A server over the seeded sample; the `TempDir` is its (empty) data dir.
async fn seeded() -> (TestServer, SampleIds, TestDb, TempDir) {
    let db = TestDb::new();
    let ids = seed_sample(&mut db.pool.get().unwrap());
    let data = tempfile::tempdir().unwrap();
    let server = TestServer::new(app(db.pool.clone(), data.path().to_path_buf()));
    (server, ids, db, data)
}

async fn post(server: &TestServer, doc: &str, vars: Value) -> Value {
    let r = server
        .post("/graphql")
        .json(&json!({ "query": doc, "variables": vars }))
        .await;
    r.assert_status_ok();
    r.json()
}

/// Runs `doc` and returns its `data`, failing on any GraphQL error.
async fn mutate(server: &TestServer, doc: &str, vars: Value) -> Value {
    let body = post(server, doc, vars).await;
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    body["data"].clone()
}

/// Runs `doc`, which must fail, and returns its first error message.
async fn mutate_err(server: &TestServer, doc: &str, vars: Value) -> String {
    let body = post(server, doc, vars).await;
    body["errors"][0]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("expected an error: {body}"))
        .to_owned()
}

/// The `name` of every object in a JSON array, in order.
fn names(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect()
}

const CREATE_ENTITY: &str = "mutation($input: EntityInput!) {
    createEntity(input: $input) {
        id name assetId parentId purchasePriceCents quantity description
        entityType { name } tags { name }
    }
}";

const UPDATE_ENTITY: &str = "mutation($id: ID!, $input: EntityInput!) {
    updateEntity(id: $id, input: $input) { id name parentId tags { name } }
}";

const DELETE_ENTITY: &str = "mutation($id: ID!) { deleteEntity(id: $id) }";

const ENTITY: &str = "query($id: ID!) { entity(id: $id) { name items { name } } }";

#[tokio::test]
async fn create_entity_returns_the_new_entity_with_an_asset_id() {
    let (server, ids, _db, _data) = seeded().await;

    let data = mutate(
        &server,
        CREATE_ENTITY,
        json!({ "input": {
            "name": "  Hammer  ",
            "description": "   ",
            "entityTypeId": ITEM_TYPE_ID,
            "parentId": ids.garage,
            "quantity": 2.0,
            "purchasePriceCents": 1234,
            "purchaseDate": "2025-03-04",
            "tagIds": [ids.tools],
        }}),
    )
    .await;

    let created = &data["createEntity"];
    assert_eq!(created["name"], "Hammer");
    assert_eq!(created["description"], Value::Null, "blank text is cleared");
    // The sample's highest asset id is 5.
    assert_eq!(created["assetId"], "000-006");
    assert_eq!(created["parentId"], ids.garage.as_str());
    assert_eq!(created["purchasePriceCents"], 1234);
    assert_eq!(created["quantity"], 2.0);
    assert_eq!(created["entityType"]["name"], "Item");
    assert_eq!(names(&created["tags"]), ["Tools"]);

    let garage = mutate(&server, ENTITY, json!({ "id": ids.garage })).await;
    assert!(
        names(&garage["entity"]["items"]).contains(&"Hammer"),
        "{garage}"
    );
}

#[tokio::test]
async fn update_entity_moves_and_retags() {
    let (server, ids, _db, _data) = seeded().await;

    let data = mutate(
        &server,
        UPDATE_ENTITY,
        json!({ "id": ids.drill, "input": {
            "name": "Cordless drill",
            "entityTypeId": ITEM_TYPE_ID,
            "parentId": ids.tote_a,
            "tagIds": [ids.electronics],
        }}),
    )
    .await;

    let updated = &data["updateEntity"];
    assert_eq!(updated["name"], "Cordless drill");
    assert_eq!(updated["parentId"], ids.tote_a.as_str());
    assert_eq!(names(&updated["tags"]), ["Electronics"]);

    let garage = mutate(&server, ENTITY, json!({ "id": ids.garage })).await;
    assert_eq!(names(&garage["entity"]["items"]), Vec::<&str>::new());
    let tote = mutate(&server, ENTITY, json!({ "id": ids.tote_a })).await;
    assert_eq!(
        names(&tote["entity"]["items"]),
        ["Cordless drill", "Screws"]
    );

    // Omitting `tagIds` leaves the tags alone.
    let data = mutate(
        &server,
        UPDATE_ENTITY,
        json!({ "id": ids.drill, "input": {
            "name": "Cordless drill",
            "entityTypeId": ITEM_TYPE_ID,
            "parentId": ids.tote_a,
        }}),
    )
    .await;
    assert_eq!(names(&data["updateEntity"]["tags"]), ["Electronics"]);
}

#[tokio::test]
async fn delete_entity_refuses_with_children_then_succeeds_after_move() {
    let (server, ids, _db, _data) = seeded().await;

    let message = mutate_err(&server, DELETE_ENTITY, json!({ "id": ids.tote_a })).await;
    assert!(
        message.contains("Tote A still contains 1 entity"),
        "{message}"
    );

    mutate(
        &server,
        UPDATE_ENTITY,
        json!({ "id": ids.screws, "input": {
            "name": "Screws",
            "entityTypeId": ITEM_TYPE_ID,
            "parentId": ids.garage,
            "quantity": 2.0,
            "purchasePriceCents": 999,
        }}),
    )
    .await;
    let data = mutate(&server, DELETE_ENTITY, json!({ "id": ids.tote_a })).await;
    assert_eq!(data["deleteEntity"], true);

    let gone = mutate(&server, ENTITY, json!({ "id": ids.tote_a })).await;
    assert_eq!(gone["entity"], Value::Null);
}

#[tokio::test]
async fn create_update_delete_entity_type() {
    let (server, ids, _db, _data) = seeded().await;

    let data = mutate(
        &server,
        "mutation($input: EntityTypeInput!) {
            createEntityType(input: $input) { id name description icon isLocation entityCount }
        }",
        json!({ "input": { "name": " Shelf ", "isLocation": true } }),
    )
    .await;
    let created = &data["createEntityType"];
    assert_eq!(created["name"], "Shelf");
    assert_eq!(created["isLocation"], true);
    assert_eq!(created["entityCount"], 0);
    let id = created["id"].as_str().unwrap().to_owned();

    let data = mutate(
        &server,
        "mutation($id: ID!, $input: EntityTypeInput!) {
            updateEntityType(id: $id, input: $input) { id name description icon isLocation }
        }",
        json!({ "id": id, "input": {
            "name": "Shelving",
            "description": "Wall shelves",
            "icon": "mdi-bookshelf",
            "isLocation": false,
        }}),
    )
    .await;
    assert_eq!(
        data["updateEntityType"],
        json!({
            "id": id,
            "name": "Shelving",
            "description": "Wall shelves",
            "icon": "mdi-bookshelf",
            "isLocation": false,
        })
    );

    let delete = "mutation($id: ID!) { deleteEntityType(id: $id) }";
    let message = mutate_err(&server, delete, json!({ "id": ids.tote_type })).await;
    assert!(
        message.contains("Tote is still used by 1 entity"),
        "{message}"
    );

    let data = mutate(&server, delete, json!({ "id": id })).await;
    assert_eq!(data["deleteEntityType"], true);
    let data = mutate(&server, "{ entityTypes { name } }", json!({})).await;
    assert_eq!(names(&data["entityTypes"]), ["Item", "Location", "Tote"]);
}

#[tokio::test]
async fn create_update_delete_tag() {
    let (server, ids, _db, _data) = seeded().await;

    let data = mutate(
        &server,
        "mutation($input: TagInput!) {
            createTag(input: $input) { id name color parent { name } entityCount }
        }",
        json!({ "input": { "name": "Power tools", "color": "#ff0000", "parentId": ids.tools } }),
    )
    .await;
    let created = &data["createTag"];
    assert_eq!(created["name"], "Power tools");
    assert_eq!(created["color"], "#ff0000");
    assert_eq!(created["parent"]["name"], "Tools");
    assert_eq!(created["entityCount"], 0);
    let id = created["id"].as_str().unwrap().to_owned();

    let data = mutate(
        &server,
        "mutation($id: ID!, $input: TagInput!) {
            updateTag(id: $id, input: $input) { id name description color icon parent { name } }
        }",
        json!({ "id": id, "input": { "name": "Cordless", "description": "Battery", "icon": "mdi-battery" } }),
    )
    .await;
    assert_eq!(
        data["updateTag"],
        json!({
            "id": id,
            "name": "Cordless",
            "description": "Battery",
            "color": null,
            "icon": "mdi-battery",
            "parent": null,
        })
    );

    // Deleting a tag in use unlinks it from its entities.
    let delete = "mutation($id: ID!) { deleteTag(id: $id) }";
    let data = mutate(&server, delete, json!({ "id": ids.tools })).await;
    assert_eq!(data["deleteTag"], true);
    let data = mutate(&server, delete, json!({ "id": id })).await;
    assert_eq!(data["deleteTag"], true);

    let data = mutate(&server, "{ tags { name } }", json!({})).await;
    assert_eq!(names(&data["tags"]), ["Electronics"]);
    let drill = mutate(
        &server,
        "query($id: ID!) { entity(id: $id) { tags { name } } }",
        json!({ "id": ids.drill }),
    )
    .await;
    assert_eq!(drill["entity"]["tags"], json!([]));
}

#[tokio::test]
async fn delete_attachment_and_set_primary_photo() {
    let (server, ids, db, data_dir) = seeded().await;
    let second_sha = "cc".repeat(32);
    diesel::insert_into(attachments::table)
        .values(fixtures::attachment(
            "a-drill-photo-2",
            &ids.drill,
            AttachmentKind::Photo,
            false,
            &second_sha,
            4,
            20,
        ))
        .execute(&mut db.pool.get().unwrap())
        .unwrap();
    let first_original = original_path(data_dir.path(), &"aa".repeat(32));
    fs::create_dir_all(first_original.parent().unwrap()).unwrap();
    fs::write(&first_original, b"jpg").unwrap();

    let set_primary = "mutation($id: ID!) {
        setPrimaryPhoto(attachmentId: $id) { id primaryPhoto { id } attachments { id primary } }
    }";
    let data = mutate(&server, set_primary, json!({ "id": "a-drill-photo-2" })).await;
    let entity = &data["setPrimaryPhoto"];
    assert_eq!(entity["id"], ids.drill.as_str());
    assert_eq!(entity["primaryPhoto"]["id"], "a-drill-photo-2");
    let primaries: Vec<&str> = entity["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["primary"] == true)
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(primaries, ["a-drill-photo-2"]);

    let message = mutate_err(&server, set_primary, json!({ "id": ids.manual })).await;
    assert!(
        message.contains("only a photo can be the primary photo"),
        "{message}"
    );

    let data = mutate(
        &server,
        "mutation($id: ID!) { deleteAttachment(id: $id) }",
        json!({ "id": ids.photo }),
    )
    .await;
    assert_eq!(data["deleteAttachment"], true);
    assert!(!first_original.exists(), "the unshared original is removed");

    let drill = mutate(
        &server,
        "query($id: ID!) { entity(id: $id) { attachments { id } } }",
        json!({ "id": ids.drill }),
    )
    .await;
    let remaining: Vec<&str> = drill["entity"]["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(remaining, ["a-drill-photo-2", ids.manual.as_str()]);
}

#[tokio::test]
async fn validation_errors_surface_as_graphql_errors() {
    let (server, ids, _db, _data) = seeded().await;

    let cases = [
        (
            CREATE_ENTITY,
            json!({ "input": { "name": "   ", "entityTypeId": ITEM_TYPE_ID } }),
            "name must not be blank",
        ),
        (
            UPDATE_ENTITY,
            json!({ "id": ids.house, "input": {
                "name": "House", "entityTypeId": LOCATION_TYPE_ID, "parentId": ids.tote_a,
            }}),
            "cannot move an entity under its own descendant",
        ),
        (
            CREATE_ENTITY,
            json!({ "input": {
                "name": "Shelf", "entityTypeId": LOCATION_TYPE_ID, "parentId": ids.drill,
            }}),
            "a location cannot be placed under an item",
        ),
        (
            CREATE_ENTITY,
            json!({ "input": {
                "name": "Saw", "entityTypeId": ITEM_TYPE_ID, "tagIds": ["t-missing"],
            }}),
            "tag not found",
        ),
        (
            CREATE_ENTITY,
            json!({ "input": {
                "name": "Saw", "entityTypeId": ITEM_TYPE_ID, "purchasePriceCents": -1,
            }}),
            "purchase price must not be negative",
        ),
        (
            CREATE_ENTITY,
            json!({ "input": {
                "name": "Saw", "entityTypeId": ITEM_TYPE_ID, "soldPriceCents": -5,
            }}),
            "sold price must not be negative",
        ),
        (
            "mutation($input: EntityTypeInput!) { createEntityType(input: $input) { id } }",
            json!({ "input": { "name": "", "isLocation": false } }),
            "name must not be blank",
        ),
        (
            "mutation($id: ID!, $input: TagInput!) { updateTag(id: $id, input: $input) { id } }",
            json!({ "id": ids.tools, "input": { "name": "Tools", "parentId": ids.electronics } }),
            "cycle",
        ),
    ];
    for (doc, vars, expected) in cases {
        let message = mutate_err(&server, doc, vars.clone()).await;
        assert!(
            message.contains(expected),
            "{vars}: expected {expected:?}, got {message:?}"
        );
    }

    // Nothing was written: the sample still has its four active items.
    let data = mutate(&server, "{ summary { totalItems } }", json!({})).await;
    assert_eq!(data["summary"]["totalItems"], 4);
}

#[tokio::test]
async fn summary_updates_after_a_create() {
    let (server, ids, _db, _data) = seeded().await;
    let summary = "{ summary { totalValueCents totalItems } }";

    let before = mutate(&server, summary, json!({})).await;
    assert_eq!(
        before["summary"],
        json!({ "totalValueCents": 67_297, "totalItems": 4 })
    );

    mutate(
        &server,
        CREATE_ENTITY,
        json!({ "input": {
            "name": "Ladder",
            "entityTypeId": ITEM_TYPE_ID,
            "parentId": ids.garage,
            "quantity": 3.0,
            "purchasePriceCents": 1000,
        }}),
    )
    .await;

    let after = mutate(&server, summary, json!({})).await;
    assert_eq!(
        after["summary"],
        json!({ "totalValueCents": 70_297, "totalItems": 5 })
    );
}
