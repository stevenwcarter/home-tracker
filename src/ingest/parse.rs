//! Reading the model's answers: the vision step's photo description (spec
//! §4.5) and the synthesis step's item suggestion (spec §4.4).
//!
//! Parsing is lenient because models drift from the schema we ask for: the
//! JSON may sit in a fenced block or among prose, numbers may arrive as
//! strings, and dates may be malformed. Anything unusable in a field becomes
//! "not found" (`None`); only an answer with no JSON object at all, or a
//! description without a summary, is an error. Nothing here panics on any
//! input, and every [`ParseError`] reads well as a photo's or item's error.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::kinds::SuggestedKind;
use crate::money::Cents;

/// Why a model answer could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("The model's answer was not valid JSON")]
    NotJson,
    #[error("The model's answer was JSON but not an object")]
    NotObject,
    #[error("The model's answer had no summary of the photo")]
    NoSummary,
}

/// What the vision step saw in one photo.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoDescription {
    pub kind: SuggestedKind,
    pub summary: String,
    /// The visible text, transcribed.
    pub text: Option<String>,
    /// Brand, model, price and the like, as the model extracted them; kept
    /// as-is for the synthesis step, which reads it as JSON.
    pub details: Value,
}

impl PhotoDescription {
    /// The description as stored in `ingest_photos.description` and shown
    /// to the synthesis model: the spec §4.5 shape, `kind` in lowercase.
    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "summary": self.summary,
            "text": self.text,
            "details": self.details,
        })
    }
}

/// How sure the model is of its suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// The synthesis step's prefilled item record (spec §4.4), stored verbatim
/// in `ingest_items.suggestion`. Every field is optional; `None` means the
/// photos did not show it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemSuggestion {
    pub name: Option<String>,
    pub description: Option<String>,
    pub manufacturer: Option<String>,
    pub model_number: Option<String>,
    pub serial_number: Option<String>,
    /// Non-negative.
    pub quantity: Option<f64>,
    pub purchase_date: Option<NaiveDate>,
    pub purchase_from: Option<String>,
    /// Integer cents of the instance currency; non-negative.
    pub purchase_price_cents: Option<Cents>,
    pub warranty_expires: Option<NaiveDate>,
    pub lifetime_warranty: Option<bool>,
    pub warranty_details: Option<String>,
    pub notes: Option<String>,
    /// Only names of tags that exist, spelled as the tag is.
    #[serde(default)]
    pub tag_names: Vec<String>,
    pub confidence: Option<Confidence>,
    pub reasoning: Option<String>,
}

/// Reads the vision step's answer. An unknown or missing `kind` is
/// [`SuggestedKind::Other`]; a missing summary is an error, since a
/// description without one tells the synthesis step nothing.
pub fn parse_description(answer: &str) -> Result<PhotoDescription, ParseError> {
    let object = first_object(answer)?;
    let summary = text(&object, "summary").ok_or(ParseError::NoSummary)?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .and_then(|kind| kind.parse().ok())
        .unwrap_or(SuggestedKind::Other);
    let details = match object.get("details") {
        Some(details @ Value::Object(_)) => details.clone(),
        _ => Value::Object(Map::new()),
    };
    Ok(PhotoDescription {
        kind,
        summary,
        text: text(&object, "text"),
        details,
    })
}

/// Reads the synthesis step's answer, keeping only the `tag_names` that
/// name one of `known_tags` (case-insensitively, respelled as the tag is).
pub fn parse_suggestion(answer: &str, known_tags: &[String]) -> Result<ItemSuggestion, ParseError> {
    let object = first_object(answer)?;
    Ok(ItemSuggestion {
        name: text(&object, "name"),
        description: text(&object, "description"),
        manufacturer: text(&object, "manufacturer"),
        model_number: text(&object, "model_number"),
        serial_number: text(&object, "serial_number"),
        quantity: number(&object, "quantity"),
        purchase_date: date(&object, "purchase_date"),
        purchase_from: text(&object, "purchase_from"),
        purchase_price_cents: cents(&object, "purchase_price_cents"),
        warranty_expires: date(&object, "warranty_expires"),
        lifetime_warranty: boolean(&object, "lifetime_warranty"),
        warranty_details: text(&object, "warranty_details"),
        notes: text(&object, "notes"),
        tag_names: known_tag_names(&object, known_tags),
        confidence: text(&object, "confidence").and_then(|raw| Confidence::parse(&raw)),
        reasoning: text(&object, "reasoning"),
    })
}

/// The JSON object in `answer`: the whole answer when it is JSON, else the
/// first balanced `{…}` group that parses as one, which skips a code fence
/// and any prose around it. JSON that is not an object (an array, a string)
/// is refused rather than searched, so `[{…}, {…}]` is not mistaken for one
/// answer. A group that never closes means a truncated answer: that is
/// [`ParseError::NotJson`], never an object nested inside it.
fn first_object(answer: &str) -> Result<Map<String, Value>, ParseError> {
    match serde_json::from_str(answer.trim()) {
        Ok(Value::Object(object)) => return Ok(object),
        Ok(_) => return Err(ParseError::NotObject),
        Err(_) => {}
    }
    let mut rest = answer;
    while let Some(start) = rest.find('{') {
        let group = &rest[start..];
        let len = braced_len(group).ok_or(ParseError::NotJson)?;
        if let Ok(Value::Object(object)) = serde_json::from_str(&group[..len]) {
            return Ok(object);
        }
        // Not JSON (prose such as `{this}`): look after the whole group, so
        // nothing nested inside it is taken for the answer.
        rest = &group[len..];
    }
    Err(ParseError::NotJson)
}

/// The byte length of the `{…}` group `text` starts with, braces inside
/// JSON strings ignored; `None` when it never closes.
fn braced_len(text: &str) -> Option<usize> {
    let mut depth = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in text.bytes().enumerate() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// A non-blank string field, trimmed.
fn text(object: &Map<String, Value>, key: &str) -> Option<String> {
    let value = object.get(key)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// A non-negative, finite number field, also accepted as a numeric string.
fn number(object: &Map<String, Value>, key: &str) -> Option<f64> {
    let value = match object.get(key)? {
        Value::Number(number) => number.as_f64()?,
        Value::String(raw) => raw.trim().parse().ok()?,
        _ => return None,
    };
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// A whole, non-negative number of cents, also accepted as a numeric string.
fn cents(object: &Map<String, Value>, key: &str) -> Option<Cents> {
    let value = number(object, key)?;
    // The guard keeps the cast exact: a whole number within i64's range.
    let whole = (value.fract() == 0.0 && value < i64::MAX as f64).then_some(value as i64)?;
    Some(Cents(whole))
}

/// An ISO `YYYY-MM-DD` date field; anything else is "not found".
fn date(object: &Map<String, Value>, key: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(object.get(key)?.as_str()?.trim(), "%Y-%m-%d").ok()
}

/// A boolean field, also accepted as the string `true` or `false`.
fn boolean(object: &Map<String, Value>, key: &str) -> Option<bool> {
    match object.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::String(raw) => raw.trim().to_ascii_lowercase().parse().ok(),
        _ => None,
    }
}

/// The `tag_names` that match one of `known_tags` ignoring case, spelled as
/// the known tag is, each at most once, in the model's order.
fn known_tag_names(object: &Map<String, Value>, known_tags: &[String]) -> Vec<String> {
    let Some(Value::Array(names)) = object.get("tag_names") else {
        return Vec::new();
    };
    let mut kept: Vec<String> = Vec::new();
    for name in names.iter().filter_map(Value::as_str) {
        let name = name.trim();
        let known = known_tags.iter().find(|tag| tag.eq_ignore_ascii_case(name));
        if let Some(tag) = known.filter(|tag| !kept.contains(tag)) {
            kept.push(tag.clone());
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESCRIPTION: &str = r#"{
        "kind": "receipt",
        "summary": "Amazon receipt for a mouse",
        "text": "Order total $99.99",
        "details": { "brand": "Logitech", "price": "99.99" }
    }"#;

    fn tags() -> Vec<String> {
        vec!["Electronics".to_owned(), "Tools".to_owned()]
    }

    #[test]
    fn a_plain_description_parses_and_round_trips_through_its_json() {
        let description = parse_description(DESCRIPTION).unwrap();
        assert_eq!(description.kind, SuggestedKind::Receipt);
        assert_eq!(description.summary, "Amazon receipt for a mouse");
        assert_eq!(description.text.as_deref(), Some("Order total $99.99"));
        assert_eq!(description.details["brand"], "Logitech");
        let stored = description.to_json();
        assert_eq!(stored["kind"], "receipt");
        assert_eq!(parse_description(&stored.to_string()), Ok(description));
    }

    #[test]
    fn a_fenced_json_block_is_read() {
        let answer = format!("```json\n{DESCRIPTION}\n```");
        assert_eq!(
            parse_description(&answer).unwrap().summary,
            "Amazon receipt for a mouse"
        );
    }

    #[test]
    fn prose_around_the_json_is_skipped() {
        let answer = format!(
            "Sure {{here}} is what I see:\n{DESCRIPTION}\nLet me know {{if}} you need more."
        );
        assert_eq!(
            parse_description(&answer).unwrap().kind,
            SuggestedKind::Receipt
        );
    }

    #[test]
    fn prose_without_json_is_not_json() {
        let err = parse_description("I see a black computer mouse on a desk.").unwrap_err();
        assert_eq!(err, ParseError::NotJson);
        assert_eq!(err.to_string(), "The model's answer was not valid JSON");
    }

    #[test]
    fn truncated_json_is_not_json() {
        assert_eq!(
            parse_suggestion(r#"{"name": "Mouse", "notes": "cut of"#, &tags()),
            Err(ParseError::NotJson)
        );
    }

    #[test]
    fn a_truncated_outer_object_never_yields_an_inner_one() {
        let truncated = r#"{"name": "Mouse", "details": {"brand": "Logitech"}, "notes": "cut of"#;
        for answer in [
            truncated.to_owned(),
            format!("```json\n{truncated}"),
            // Cut mid-structure, then the fence closes: still one open group.
            "```json\n{\"summary\": \"A mouse\", \"details\": {\"summary\": \"inner\"}\n```"
                .to_owned(),
        ] {
            assert_eq!(
                parse_suggestion(&answer, &tags()),
                Err(ParseError::NotJson),
                "{answer}"
            );
            assert_eq!(
                parse_description(&answer),
                Err(ParseError::NotJson),
                "{answer}"
            );
        }
    }

    #[test]
    fn braces_inside_strings_do_not_end_the_object() {
        let answer = r#"Here: {"summary": "A sign reading \"}{\" and {x}", "kind": "photo"} done"#;
        assert_eq!(
            parse_description(answer).unwrap().summary,
            r#"A sign reading "}{" and {x}"#
        );
    }

    #[test]
    fn non_object_json_is_refused() {
        for answer in [r#"[{"summary": "a"}]"#, r#""a mouse""#, "42", "null"] {
            assert_eq!(
                parse_description(answer),
                Err(ParseError::NotObject),
                "{answer}"
            );
        }
    }

    #[test]
    fn a_description_needs_a_summary_but_not_a_known_kind() {
        assert_eq!(
            parse_description(r#"{"kind": "receipt", "summary": "  "}"#),
            Err(ParseError::NoSummary)
        );
        let lenient =
            parse_description(r#"{"kind": "selfie", "summary": "A mouse", "details": 3}"#).unwrap();
        assert_eq!(lenient.kind, SuggestedKind::Other);
        assert_eq!(lenient.text, None);
        assert_eq!(lenient.details, json!({}));
    }

    #[test]
    fn numeric_strings_are_coerced() {
        let suggestion = parse_suggestion(
            r#"{"quantity": "2", "purchase_price_cents": " 9999 ", "lifetime_warranty": "true"}"#,
            &tags(),
        )
        .unwrap();
        assert_eq!(suggestion.quantity, Some(2.0));
        assert_eq!(suggestion.purchase_price_cents, Some(Cents(9999)));
        assert_eq!(suggestion.lifetime_warranty, Some(true));
    }

    #[test]
    fn unusable_numbers_are_not_found() {
        let suggestion = parse_suggestion(
            r#"{"quantity": -1, "purchase_price_cents": "99.99"}"#,
            &tags(),
        )
        .unwrap();
        assert_eq!(suggestion.quantity, None);
        assert_eq!(suggestion.purchase_price_cents, None);
        let suggestion = parse_suggestion(
            r#"{"quantity": "many", "purchase_price_cents": 1e300}"#,
            &tags(),
        )
        .unwrap();
        assert_eq!(suggestion.quantity, None);
        assert_eq!(suggestion.purchase_price_cents, None);
    }

    #[test]
    fn bad_dates_are_dropped() {
        let suggestion = parse_suggestion(
            r#"{"purchase_date": "March 2024", "warranty_expires": "2026-02-30"}"#,
            &tags(),
        )
        .unwrap();
        assert_eq!(suggestion.purchase_date, None);
        assert_eq!(suggestion.warranty_expires, None);
        let suggestion = parse_suggestion(r#"{"purchase_date": "2024-03-12"}"#, &tags()).unwrap();
        assert_eq!(
            suggestion.purchase_date,
            NaiveDate::from_ymd_opt(2024, 3, 12)
        );
    }

    #[test]
    fn wrong_types_are_not_found() {
        let suggestion = parse_suggestion(
            r#"{"name": 7, "lifetime_warranty": 1, "tag_names": "Tools", "confidence": "sure"}"#,
            &tags(),
        )
        .unwrap();
        assert_eq!(suggestion, ItemSuggestion::default());
    }

    #[test]
    fn tag_names_are_filtered_to_known_tags_case_insensitively() {
        let suggestion = parse_suggestion(
            r#"{"tag_names": ["electronics", "Garden", "TOOLS", "Electronics", 3]}"#,
            &tags(),
        )
        .unwrap();
        assert_eq!(suggestion.tag_names, ["Electronics", "Tools"]);
    }

    #[test]
    fn a_suggestion_round_trips_through_its_stored_json() {
        let suggestion = parse_suggestion(
            r#"{"name": "Mouse", "quantity": 1, "purchase_date": "2024-03-12",
                "purchase_price_cents": 9999, "tag_names": ["Tools"], "confidence": "HIGH"}"#,
            &tags(),
        )
        .unwrap();
        assert_eq!(suggestion.confidence, Some(Confidence::High));
        let stored = serde_json::to_value(&suggestion).unwrap();
        assert_eq!(stored["purchase_price_cents"], 9999);
        assert_eq!(stored["purchase_date"], "2024-03-12");
        assert_eq!(stored["confidence"], "high");
        assert_eq!(stored["serial_number"], Value::Null);
        assert_eq!(
            serde_json::from_value::<ItemSuggestion>(stored).unwrap(),
            suggestion
        );
    }
}
