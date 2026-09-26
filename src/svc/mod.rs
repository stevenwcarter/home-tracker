//! Business logic. Each module owns one aggregate and takes a `&mut SqliteConnection`.

use anyhow::{Result, ensure};

pub mod attachment;
pub mod entity;
pub mod entity_field;
pub mod entity_type;
pub mod fixtures;
pub mod settings;
pub mod sniff;
pub mod stats;
pub mod tag;
pub mod thumbnail;
pub mod thumbnail_service;
pub mod upload;

/// `name` with surrounding whitespace removed; blank names are refused.
/// Every named aggregate (entities, types, tags) goes through this.
pub(crate) fn required_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    ensure!(!trimmed.is_empty(), "name must not be blank");
    Ok(trimmed.to_owned())
}

/// Optional free text with surrounding whitespace removed; blank becomes
/// `None`, so an emptied form field clears the column instead of storing `""`.
pub fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

/// `"1 entity"` / `"3 entities"`, for user-facing refusal messages.
pub(crate) fn entity_count_phrase(n: i64) -> String {
    if n == 1 {
        "1 entity".to_owned()
    } else {
        format!("{n} entities")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_text_trims_and_drops_blanks() {
        assert_eq!(optional_text(None), None);
        assert_eq!(optional_text(Some(String::new())), None);
        assert_eq!(optional_text(Some(" \t\n ".to_owned())), None);
        assert_eq!(
            optional_text(Some("  a note ".to_owned())).as_deref(),
            Some("a note")
        );
        assert_eq!(
            optional_text(Some("kept".to_owned())).as_deref(),
            Some("kept")
        );
    }
}
