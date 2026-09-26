//! The system prompts and the JSON schemas the model answers against.
//!
//! The vision step describes one photo as a [`photo_description_schema`]
//! object (spec §4.5); the synthesis step combines an item's descriptions
//! into an [`item_suggestion_schema`] object (spec §4.4). Both schemas are
//! strict-mode compatible: every property is listed in `required`, "not
//! found" is `null`, and no other properties are allowed.

use serde_json::{Map, Value, json};

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn extra_instructions_are_a_final_paragraph() {
        assert_eq!(
            with_extra_instructions("Base.", Some("  Prefer metric units. ")),
            "Base.\n\nAdditional instructions from the user:\nPrefer metric units."
        );
        assert_eq!(with_extra_instructions("Base.", None), "Base.");
        assert_eq!(with_extra_instructions("Base.", Some("  ")), "Base.");
    }
}
