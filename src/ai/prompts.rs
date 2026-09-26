//! The system prompts and the JSON schemas the model answers against.
//!
//! The vision step describes one photo as a [`photo_description_schema`]
//! object (spec §4.5); the synthesis step combines an item's descriptions
//! into an [`item_suggestion_schema`] object (spec §4.4). Both schemas are
//! strict-mode compatible: every property is listed in `required`, "not
//! found" is `null`, and no other properties are allowed.

use std::fmt::Write as _;

use serde_json::{Map, Value, json};

use crate::ingest::parse::PhotoDescription;

/// The system prompt for describing one photo.
pub const VISION_SYSTEM: &str = "\
You describe one photograph of a household item, or of a document about one \
(a receipt, a warranty card, a manual page), for a home inventory.

Classify the photo's kind as one of: photo, receipt, warranty, manual, other. \
Write a one-sentence summary. Transcribe the visible text exactly as written, \
in reading order. Extract the item name, brand, model number, serial number, \
price, currency, date, vendor and warranty terms when they are visible.

Never invent a value: use null for anything the photo does not show, and \
never guess a serial number, model number or price. \
Answer only with a JSON object matching the given schema, with no other text.";

/// The system prompt for combining an item's photo descriptions.
pub const SYNTHESIS_SYSTEM: &str = "\
You fill in one household item's inventory record from descriptions of its \
photos. You are given the descriptions, the field list, the instance currency \
and the existing tags.

Use null for any field the descriptions do not support. Never invent a value: \
never make up a serial number, model number or price. Write dates as \
YYYY-MM-DD. Write prices as integer cents of the instance currency. Choose \
tag_names only from the existing tags given. Set confidence to low, medium or \
high, and explain briefly in reasoning where each value came from.

Answer only with a JSON object matching the given schema, with no other text.";

/// A nullable property of JSON type `kind`.
fn nullable(kind: &str) -> Value {
    json!({ "type": [kind, "null"] })
}

/// A strict object schema: every property required, nothing else allowed.
fn object(properties: Vec<(&str, Value)>) -> Value {
    let required: Vec<&str> = properties.iter().map(|(name, _)| *name).collect();
    let properties: Map<String, Value> = properties
        .into_iter()
        .map(|(name, schema)| (name.to_owned(), schema))
        .collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// The schema of one photo's description (spec §4.5).
pub fn photo_description_schema() -> Value {
    let details = object(
        [
            "item_name",
            "brand",
            "model_number",
            "serial_number",
            "price",
            "currency",
            "date",
            "vendor",
            "warranty",
            "other",
        ]
        .into_iter()
        .map(|name| (name, nullable("string")))
        .collect(),
    );
    object(vec![
        (
            "kind",
            json!({ "type": "string", "enum": ["photo", "receipt", "warranty", "manual", "other"] }),
        ),
        ("summary", json!({ "type": "string" })),
        ("text", nullable("string")),
        ("details", details),
    ])
}

/// The schema of an item suggestion (spec §4.4); every field may be `null`.
pub fn item_suggestion_schema() -> Value {
    let text = |name| (name, nullable("string"));
    let date = |name| {
        (
            name,
            json!({ "type": ["string", "null"], "description": "YYYY-MM-DD" }),
        )
    };
    object(vec![
        text("name"),
        text("description"),
        text("manufacturer"),
        text("model_number"),
        text("serial_number"),
        (
            "quantity",
            json!({ "type": ["number", "null"], "minimum": 0 }),
        ),
        date("purchase_date"),
        text("purchase_from"),
        (
            "purchase_price_cents",
            json!({ "type": ["integer", "null"], "description": "integer cents of the instance currency" }),
        ),
        date("warranty_expires"),
        ("lifetime_warranty", nullable("boolean")),
        text("warranty_details"),
        text("notes"),
        (
            "tag_names",
            json!({ "type": ["array", "null"], "items": { "type": "string" } }),
        ),
        (
            "confidence",
            json!({ "type": ["string", "null"], "enum": ["low", "medium", "high", null] }),
        ),
        text("reasoning"),
    ])
}

/// `base` with the user's extra instructions, when there are any, appended
/// as its final paragraph.
pub fn with_extra_instructions(base: &str, extra: Option<&str>) -> String {
    match extra.map(str::trim).filter(|extra| !extra.is_empty()) {
        Some(extra) => format!("{base}\n\nAdditional instructions from the user:\n{extra}"),
        None => base.to_owned(),
    }
}

/// The item record's fields and their types, as the synthesis message lists
/// them; the same fields as [`item_suggestion_schema`] (a test pins that).
const SUGGESTION_FIELDS: [(&str, &str); 16] = [
    ("name", "text"),
    ("description", "text"),
    ("manufacturer", "text"),
    ("model_number", "text"),
    ("serial_number", "text"),
    ("quantity", "number, at least 0"),
    ("purchase_date", "date, YYYY-MM-DD"),
    ("purchase_from", "text"),
    (
        "purchase_price_cents",
        "integer cents of the instance currency",
    ),
    ("warranty_expires", "date, YYYY-MM-DD"),
    ("lifetime_warranty", "true or false"),
    ("warranty_details", "text"),
    ("notes", "text"),
    ("tag_names", "list of names from the existing tags"),
    ("confidence", "low, medium or high"),
    ("reasoning", "text"),
];

/// The user message beside photo `position` (1-based) of an item's `total`.
pub fn describe_user_message(position: usize, total: usize) -> String {
    format!("Photo {position} of {total} of one item.")
}

/// The synthesis step's user message: each described photo (by its 1-based
/// position among the item's photos) as JSON, the instance currency, the
/// existing tag names and the fields to fill.
pub fn synthesis_user_message(
    descriptions: &[(usize, &PhotoDescription)],
    currency: &str,
    tag_names: &[String],
) -> String {
    let mut message = String::from("Descriptions of the item's photos:\n");
    for (position, description) in descriptions {
        // Writing to a String cannot fail.
        let _ = writeln!(message, "Photo {position}: {}", description.to_json());
    }
    let _ = write!(
        message,
        "\nInstance currency: {currency}\nExisting tags: {}\n\nFields (null when not found):\n",
        Value::from(tag_names)
    );
    for (name, kind) in SUGGESTION_FIELDS {
        let _ = writeln!(message, "- {name}: {kind}");
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::SuggestedKind;

    /// The property names of object schema `schema`, sorted.
    fn properties(schema: &Value) -> Vec<&str> {
        let mut names: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        names.sort_unstable();
        names
    }

    /// `schema` lists exactly `expected` as properties, all required, no others.
    fn assert_strict_object(schema: &Value, expected: &[&str]) {
        let mut expected = expected.to_vec();
        expected.sort_unstable();
        assert_eq!(properties(schema), expected);
        let mut required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        required.sort_unstable();
        assert_eq!(required, expected);
        assert_eq!(schema["additionalProperties"], json!(false));
    }

    #[test]
    fn each_system_prompt_asks_for_json_only_and_never_inventing() {
        for prompt in [VISION_SYSTEM, SYNTHESIS_SYSTEM] {
            assert!(
                prompt.contains("Answer only with a JSON object"),
                "{prompt}"
            );
            assert!(prompt.contains("Never invent a value"), "{prompt}");
            assert!(prompt.contains("null"), "{prompt}");
        }
        assert!(SYNTHESIS_SYSTEM.contains("integer cents of the instance currency"));
        assert!(SYNTHESIS_SYSTEM.contains("only from the existing tags"));
        assert!(VISION_SYSTEM.contains("Transcribe the visible text exactly"));
    }

    #[test]
    fn the_photo_schema_lists_every_field_of_spec_4_5() {
        let schema = photo_description_schema();
        assert_strict_object(&schema, &["kind", "summary", "text", "details"]);
        assert_strict_object(
            &schema["properties"]["details"],
            &[
                "item_name",
                "brand",
                "model_number",
                "serial_number",
                "price",
                "currency",
                "date",
                "vendor",
                "warranty",
                "other",
            ],
        );
        assert_eq!(
            schema["properties"]["kind"]["enum"],
            json!(["photo", "receipt", "warranty", "manual", "other"])
        );
    }

    #[test]
    fn the_suggestion_schema_lists_every_field_of_spec_4_4() {
        let schema = item_suggestion_schema();
        assert_strict_object(
            &schema,
            &[
                "name",
                "description",
                "manufacturer",
                "model_number",
                "serial_number",
                "quantity",
                "purchase_date",
                "purchase_from",
                "purchase_price_cents",
                "warranty_expires",
                "lifetime_warranty",
                "warranty_details",
                "notes",
                "tag_names",
                "confidence",
                "reasoning",
            ],
        );
        // Every field is optional: `null` means "not found".
        for (name, property) in schema["properties"].as_object().unwrap() {
            assert!(
                property["type"]
                    .as_array()
                    .is_some_and(|types| types.contains(&json!("null"))),
                "{name} is not nullable"
            );
        }
    }

    #[test]
    fn the_synthesis_field_list_matches_the_schema() {
        let listed: Vec<&str> = SUGGESTION_FIELDS.iter().map(|(name, _)| *name).collect();
        assert_strict_object(&item_suggestion_schema(), &listed);
    }

    #[test]
    fn the_describe_message_places_the_photo() {
        assert_eq!(describe_user_message(2, 3), "Photo 2 of 3 of one item.");
    }

    #[test]
    fn the_synthesis_message_holds_every_summary_the_currency_and_every_tag() {
        let receipt = PhotoDescription {
            kind: SuggestedKind::Receipt,
            summary: "Amazon receipt for a mouse".to_owned(),
            text: Some("Total 99.99".to_owned()),
            details: json!({ "price": "99.99" }),
        };
        let label = PhotoDescription {
            kind: SuggestedKind::Photo,
            summary: "Underside label of a mouse".to_owned(),
            text: None,
            details: json!({}),
        };
        let tags = ["Electronics".to_owned(), "Home \"office\"".to_owned()];
        let message = synthesis_user_message(&[(1, &receipt), (3, &label)], "EUR", &tags);
        for needle in [
            "Photo 1: ",
            "Amazon receipt for a mouse",
            "Total 99.99",
            "\"receipt\"",
            "Photo 3: ",
            "Underside label of a mouse",
            "Instance currency: EUR",
            "Electronics",
            r#"Home \"office\""#,
            "- purchase_price_cents: integer cents of the instance currency",
            "- reasoning: text",
        ] {
            assert!(
                message.contains(needle),
                "{needle:?} missing from {message}"
            );
        }
    }

    #[test]
    fn extra_instructions_are_a_final_paragraph() {
        assert_eq!(
            with_extra_instructions("Base.", Some("  Prefer metric units. ")),
            "Base.\n\nAdditional instructions from the user:\nPrefer metric units."
        );
        assert_eq!(with_extra_instructions("Base.", None), "Base.");
        assert_eq!(with_extra_instructions("Base.", Some("  ")), "Base.");
    }
}
