use axum_test::TestServer;
use chrono::DateTime;
use diesel::prelude::*;
use home_tracker::db::{ITEM_TYPE_ID, TestDb};
use home_tracker::routes::app;
use home_tracker::schema::{attachments, entities};
use home_tracker::svc::fixtures::{self, SampleIds, seed_sample};
use serde_json::{Value, json};

async fn seeded() -> (TestServer, SampleIds, TestDb) {
    let db = TestDb::new();
    let ids = seed_sample(&mut db.pool.get().unwrap());
    let server = TestServer::new(app(db.pool.clone()));
    (server, ids, db)
}

async fn query(server: &TestServer, q: &str, vars: Value) -> Value {
    let r = server
        .post("/graphql")
        .json(&json!({ "query": q, "variables": vars }))
        .await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    body["data"].clone()
}

/// The `name` of every object in a JSON array, in order.
fn names(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn entity_types_lists_all_with_counts() {
    let (server, _ids, _db) = seeded().await;

    let data = query(
        &server,
        "{ entityTypes { name isLocation entityCount } }",
        json!({}),
    )
    .await;

    assert_eq!(
        data["entityTypes"],
        json!([
            { "name": "Item", "isLocation": false, "entityCount": 5 },
            { "name": "Location", "isLocation": true, "entityCount": 3 },
            { "name": "Tote", "isLocation": true, "entityCount": 1 },
        ])
    );
}

#[tokio::test]
async fn locations_is_flat_with_parent_ids() {
    let (server, ids, _db) = seeded().await;

    let data = query(
        &server,
        "{ locations { id name parentId isLocation } }",
        json!({}),
    )
    .await;

    let rows = data["locations"].as_array().unwrap();
    assert_eq!(
        names(&data["locations"]),
        ["Attic", "Garage", "House", "Tote A"]
    );
    assert!(
        rows.iter().all(|r| r["isLocation"] == true),
        "not every row is a location: {rows:?}"
    );
    let by_name = |name: &str| rows.iter().find(|r| r["name"] == name).unwrap();
    assert_eq!(by_name("Garage")["parentId"], ids.house);
    assert_eq!(by_name("House")["parentId"], Value::Null);
}

#[tokio::test]
async fn entity_returns_every_scalar_and_relationship() {
    let (server, ids, _db) = seeded().await;

    let data = query(
        &server,
        "query($id: ID!) { entity(id: $id) {
            id name description
            entityType { id name description icon isLocation entityCount createdAt updatedAt }
            isLocation
            parent { name }
            ancestors { name }
            childLocations { name }
            items { name }
            archived assetId quantity insured
            serialNumber modelNumber manufacturer notes
            lifetimeWarranty warrantyExpires warrantyDetails
            purchaseDate purchaseFrom purchasePriceCents
            soldDate soldTo soldPriceCents soldNotes
            tags { id name description color icon parent { name } entityCount }
            attachments { id kind primary title mimeType sizeBytes url thumbnailUrl }
            primaryPhoto { url thumbnailUrl }
            fields { id name kind textValue numberValue booleanValue timeValue }
            createdAt updatedAt
        } }",
        json!({ "id": ids.drill }),
    )
    .await;

    let drill = &data["entity"];
    assert_eq!(drill["id"], ids.drill);
    assert_eq!(drill["name"], "Drill");
    assert_eq!(drill["assetId"], "000-003");
    assert_eq!(drill["purchasePriceCents"], 15_299);
    assert_eq!(drill["quantity"], 1.0);
    assert_eq!(drill["isLocation"], false);
    assert_eq!(drill["entityType"]["name"], "Item");
    assert_eq!(drill["parent"]["name"], "Garage");
    assert_eq!(names(&drill["ancestors"]), ["House", "Garage"]);
    assert_eq!(drill["tags"][0]["name"], "Tools");

    let attachments = drill["attachments"].as_array().unwrap();
    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0]["id"], ids.photo);
    assert_eq!(attachments[0]["kind"], "PHOTO");
    assert_eq!(attachments[0]["primary"], true);
    assert_eq!(attachments[1]["id"], ids.manual);
    assert_eq!(attachments[1]["mimeType"], "application/pdf");
    assert_eq!(attachments[1]["thumbnailUrl"], Value::Null);

    assert_eq!(
        drill["primaryPhoto"]["url"],
        "/attachments/a-drill-photo?v=aaaaaaaaaaaa"
    );
    assert_eq!(
        drill["primaryPhoto"]["thumbnailUrl"],
        "/attachments/a-drill-photo/thumb/500?v=aaaaaaaaaaaa"
    );

    assert_eq!(drill["fields"][0]["name"], "Voltage");
    assert_eq!(drill["fields"][0]["numberValue"], 18);

    let created_at = drill["createdAt"].as_str().unwrap();
    assert!(
        DateTime::parse_from_rfc3339(created_at).is_ok(),
        "createdAt is not RFC 3339: {created_at}"
    );
}

#[tokio::test]
async fn entity_child_locations_and_items() {
    let (server, ids, _db) = seeded().await;

    let data = query(
        &server,
        "query($id: ID!) { entity(id: $id) { childLocations { name } items { name } } }",
        json!({ "id": ids.garage }),
    )
    .await;

    assert_eq!(names(&data["entity"]["childLocations"]), ["Tote A"]);
    assert_eq!(names(&data["entity"]["items"]), ["Drill"]);
}

#[tokio::test]
async fn entity_returns_null_for_unknown_id() {
    let (server, _ids, _db) = seeded().await;

    let data = query(
        &server,
        "query($id: ID!) { entity(id: $id) { name } }",
        json!({ "id": "no-such-entity" }),
    )
    .await;

    assert_eq!(data["entity"], Value::Null);
}

#[tokio::test]
async fn root_items_lists_parentless_items() {
    let (server, _ids, _db) = seeded().await;

    let data = query(&server, "{ rootItems { name } }", json!({})).await;

    assert_eq!(names(&data["rootItems"]), ["Loose item"]);
}

#[tokio::test]
async fn tags_list_with_parent_and_count() {
    let (server, _ids, _db) = seeded().await;

    let data = query(
        &server,
        "{ tags { name parent { name } entityCount } }",
        json!({}),
    )
    .await;

    let tags = data["tags"].as_array().unwrap();
    let by_name = |name: &str| tags.iter().find(|t| t["name"] == name).unwrap();
    assert_eq!(by_name("Electronics")["parent"]["name"], "Tools");
    assert_eq!(by_name("Tools")["parent"], Value::Null);
    assert_eq!(by_name("Tools")["entityCount"], 1);
}

#[tokio::test]
async fn search_finds_by_substring_with_limit() {
    let (server, _ids, _db) = seeded().await;

    let limited = query(
        &server,
        r#"{ search(query: "e", limit: 2) { name } }"#,
        json!({}),
    )
    .await;
    assert_eq!(limited["search"].as_array().unwrap().len(), 2);

    let drill = query(&server, r#"{ search(query: "drill") { name } }"#, json!({})).await;
    assert_eq!(names(&drill["search"]), ["Drill"]);
}

#[tokio::test]
async fn search_limit_is_clamped() {
    let db = TestDb::new();
    {
        let mut conn = db.pool.get().unwrap();
        for n in 0..250 {
            let id = format!("bulk-{n}");
            diesel::insert_into(entities::table)
                .values(fixtures::entity(&id, &id, ITEM_TYPE_ID, None, n))
                .execute(&mut conn)
                .unwrap();
        }
    }
    let server = TestServer::new(app(db.pool.clone()));

    let hits = |limit: i32| {
        let server = &server;
        async move {
            let q = format!(r#"{{ search(query: "bulk", limit: {limit}) {{ id }} }}"#);
            let data = query(server, &q, json!({})).await;
            data["search"].as_array().unwrap().len()
        }
    };
    assert_eq!(hits(1000).await, 200);
    assert_eq!(hits(0).await, 1);
}

#[tokio::test]
async fn attachment_urls_carry_a_version_tag() {
    let (server, ids, _db) = seeded().await;

    let data = query(
        &server,
        "query($id: ID!) { entity(id: $id) { primaryPhoto { url thumbnailUrl(size: 300) } } }",
        json!({ "id": ids.drill }),
    )
    .await;

    // The fixture's photo sha256 is "aa"×32; the version tag is its first 12 chars.
    let photo = &data["entity"]["primaryPhoto"];
    assert_eq!(photo["url"], "/attachments/a-drill-photo?v=aaaaaaaaaaaa");
    assert_eq!(
        photo["thumbnailUrl"],
        "/attachments/a-drill-photo/thumb/300?v=aaaaaaaaaaaa"
    );
}

#[tokio::test]
async fn thumbnail_url_is_null_for_non_images_and_case_insensitive() {
    let (server, ids, db) = seeded().await;
    {
        let mut conn = db.pool.get().unwrap();
        diesel::update(attachments::table.find(&ids.manual))
            .set(attachments::mime_type.eq("application/PDF"))
            .execute(&mut conn)
            .unwrap();
        diesel::update(attachments::table.find(&ids.photo))
            .set(attachments::mime_type.eq("image/JPEG"))
            .execute(&mut conn)
            .unwrap();
    }

    let data = query(
        &server,
        "query($id: ID!) { entity(id: $id) { attachments { id thumbnailUrl } } }",
        json!({ "id": ids.drill }),
    )
    .await;

    let rows = data["entity"]["attachments"].as_array().unwrap();
    let by_id = |id: &str| rows.iter().find(|r| r["id"] == id).unwrap();
    assert_eq!(by_id(&ids.manual)["thumbnailUrl"], Value::Null);
    assert!(by_id(&ids.photo)["thumbnailUrl"].is_string());
}

#[tokio::test]
async fn argument_types_and_defaults_match_the_spec() {
    let (server, _ids, _db) = seeded().await;

    let data = query(
        &server,
        r#"{
            query: __type(name: "Query") { fields { name args { name defaultValue type { kind name ofType { name } } } } }
            attachment: __type(name: "Attachment") { fields { name args { name defaultValue type { kind name ofType { name } } } } }
            entity: __type(name: "Entity") { fields { name type { kind name ofType { name } } } }
        }"#,
        json!({}),
    )
    .await;

    let arg = |ty: &str, field: &str, arg: &str| {
        let fields = data[ty]["fields"].as_array().unwrap();
        let f = fields.iter().find(|f| f["name"] == field).unwrap();
        let args = f["args"].as_array().unwrap();
        args.iter().find(|a| a["name"] == arg).unwrap().clone()
    };

    let limit = arg("query", "search", "limit");
    assert_eq!(
        limit["type"]["kind"], "SCALAR",
        "limit is nullable: {limit}"
    );
    assert_eq!(limit["type"]["name"], "Int");
    assert_eq!(limit["defaultValue"], "25");

    let size = arg("attachment", "thumbnailUrl", "size");
    assert_eq!(size["type"]["kind"], "NON_NULL", "size is Int!: {size}");
    assert_eq!(size["type"]["ofType"]["name"], "Int");
    assert_eq!(size["defaultValue"], "500");

    let id = arg("query", "entity", "id");
    assert_eq!(id["type"]["ofType"]["name"], "ID");

    // `locationTree`/`LocationNode` are retired in favour of a flat `locations`
    // query the client nests itself via `Entity.parentId`.
    let query_fields: Vec<&str> = data["query"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(query_fields.contains(&"locations"), "{query_fields:?}");
    assert!(!query_fields.contains(&"locationTree"), "{query_fields:?}");

    let entity_fields = data["entity"]["fields"].as_array().unwrap();
    let field_type = |name: &str| {
        let f = entity_fields.iter().find(|f| f["name"] == name).unwrap();
        f["type"].clone()
    };
    assert_eq!(field_type("purchaseDate")["name"], "LocalDate");
    assert_eq!(field_type("createdAt")["ofType"]["name"], "DateTime");
    assert_eq!(field_type("quantity")["ofType"]["name"], "Float");
    assert_eq!(field_type("id")["ofType"]["name"], "ID");
    assert_eq!(
        field_type("parentId")["kind"],
        "SCALAR",
        "parentId is nullable"
    );
    assert_eq!(field_type("parentId")["name"], "ID");
}
