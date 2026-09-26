//! Business logic. Each module owns one aggregate and takes a `&mut SqliteConnection`.

use anyhow::{Result, ensure};

pub mod attachment;
pub mod entity;
pub mod entity_field;
pub mod entity_type;
pub mod fixtures;
pub mod settings;
pub mod stats;
pub mod tag;
pub mod thumbnail;
pub mod thumbnail_service;

/// `name` with surrounding whitespace removed; blank names are refused.
/// Every named aggregate (entities, types, tags) goes through this.
pub(crate) fn required_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    ensure!(!trimmed.is_empty(), "name must not be blank");
    Ok(trimmed.to_owned())
}

/// `"1 entity"` / `"3 entities"`, for user-facing refusal messages.
pub(crate) fn entity_count_phrase(n: i64) -> String {
    if n == 1 {
        "1 entity".to_owned()
    } else {
        format!("{n} entities")
    }
}
