use chrono::{DateTime, Utc};
use juniper::ID;

use super::saturating_int;
use crate::graphql::context::GraphQLContext;
use crate::kinds::FieldKind;
use crate::models::EntityField;

/// A user-defined custom field on an entity.
#[juniper::graphql_object(context = GraphQLContext, name = "EntityField")]
impl EntityField {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn kind(&self) -> FieldKind {
        self.kind
    }
    fn text_value(&self) -> Option<&str> {
        self.text_value.as_deref()
    }
    fn number_value(&self) -> Option<i32> {
        self.number_value.map(saturating_int)
    }
    fn boolean_value(&self) -> bool {
        self.boolean_value
    }
    fn time_value(&self) -> Option<DateTime<Utc>> {
        self.time_value.map(|t| t.and_utc())
    }
}
