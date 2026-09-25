use anyhow::Context as _;
use juniper::FieldResult;

use super::context::GraphQLContext;
use super::schema::graphql_translate_anyhow;
use crate::svc::stats::{self, Summary};

pub struct Query;

#[juniper::graphql_object(context = GraphQLContext)]
impl Query {
    /// The home-page quick statistics.
    fn summary(context: &GraphQLContext) -> FieldResult<Summary> {
        graphql_translate_anyhow(
            context
                .pool
                .get()
                .context("could not get a database connection")
                .and_then(|mut conn| stats::summary(&mut conn)),
        )
    }
}
