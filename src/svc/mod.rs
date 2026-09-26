//! Business logic. Each module owns one aggregate and takes a `&mut SqliteConnection`.

pub mod attachment;
pub mod entity;
pub mod entity_field;
pub mod entity_type;
pub mod fixtures;
pub mod settings;
pub mod stats;
pub mod tag;
pub mod thumbnail;
