use juniper::{FieldResult, ID};

use super::saturating_int;
use crate::graphql::context::GraphQLContext;
use crate::graphql::schema::graphql_translate_anyhow as gql;
use crate::models::Tag;
use crate::svc;

/// A label on entities; tags may nest under a parent tag.
#[juniper::graphql_object(context = GraphQLContext, name = "Tag")]
impl Tag {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    fn color(&self) -> Option<&str> {
        self.color.as_deref()
    }
    fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }
    fn parent(&self, ctx: &GraphQLContext) -> FieldResult<Option<Tag>> {
        match &self.parent_id {
            None => Ok(None),
            Some(pid) => gql(ctx.conn().and_then(|mut c| svc::tag::get(&mut c, pid))),
        }
    }
    /// How many entities carry this tag.
    fn entity_count(&self, ctx: &GraphQLContext) -> FieldResult<i32> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::tag::entity_count(&mut c, &self.id)))
        .map(saturating_int)
    }
}
