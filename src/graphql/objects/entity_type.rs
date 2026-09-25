use chrono::{DateTime, Utc};
use juniper::{FieldResult, ID};

use super::saturating_int;
use crate::graphql::context::GraphQLContext;
use crate::graphql::schema::graphql_translate_anyhow as gql;
use crate::models::EntityType;
use crate::svc;

/// A kind of entity; `isLocation` decides whether it can hold other entities.
#[juniper::graphql_object(context = GraphQLContext, name = "EntityType")]
impl EntityType {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }
    fn is_location(&self) -> bool {
        self.is_location
    }
    /// How many entities have this type.
    fn entity_count(&self, ctx: &GraphQLContext) -> FieldResult<i32> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity_type::entity_count(&mut c, &self.id)))
        .map(saturating_int)
    }
    fn created_at(&self) -> DateTime<Utc> {
        self.created_at.and_utc()
    }
    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at.and_utc()
    }
}
