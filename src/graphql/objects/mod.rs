//! GraphQL object impls over the inventory models.
//!
//! Each file implements `juniper::graphql_object` directly on a
//! `crate::models` row, so resolvers read fields without a copy and fetch
//! relationships lazily through `svc`.

mod attachment;
mod entity;
mod entity_field;
mod entity_type;
mod tag;

/// A GraphQL `Int` from a database count or size, saturating at `i32::MAX`
/// (the counts here never approach it; saturating beats an error).
fn saturating_int(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}
