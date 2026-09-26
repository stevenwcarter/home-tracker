use juniper::{EmptySubscription, FieldError, FieldResult, RootNode};
use tracing::error;

use super::context::GraphQLContext;
use super::mutation::Mutation;
use super::query::Query;

pub type Schema = RootNode<Query, Mutation, EmptySubscription<GraphQLContext>>;

pub fn create_schema() -> Schema {
    Schema::new(Query, Mutation, EmptySubscription::new())
}

/// Converts an `anyhow::Result` into a juniper `FieldResult`, logging failures.
pub fn graphql_translate_anyhow<T>(res: anyhow::Result<T>) -> FieldResult<T> {
    res.map_err(|e| {
        error!("GraphQL error: {e:#}");
        FieldError::from(e)
    })
}
