use juniper::{FieldResult, ID};

use super::context::GraphQLContext;
use super::schema::graphql_translate_anyhow as gql;
use crate::models::{Entity, EntityType, Tag};
use crate::svc;
use crate::svc::stats::Summary;

/// `search` returns this many rows when the caller gives no limit.
const DEFAULT_SEARCH_LIMIT: i32 = 25;
/// The most rows one `search` may return, whatever the caller asks for.
const MAX_SEARCH_LIMIT: i32 = 200;

pub struct Query;

#[juniper::graphql_object(context = GraphQLContext)]
impl Query {
    /// The home-page quick statistics.
    fn summary(context: &GraphQLContext) -> FieldResult<Summary> {
        gql(context.conn().and_then(|mut c| svc::stats::summary(&mut c)))
    }

    /// Every entity type, by name.
    fn entity_types(context: &GraphQLContext) -> FieldResult<Vec<EntityType>> {
        gql(context
            .conn()
            .and_then(|mut c| svc::entity_type::list(&mut c)))
    }

    /// Every location, flat and sorted by name; the client nests them into a
    /// tree using `parentId` (a GraphQL selection cannot recurse, so a fixed-
    /// depth tree query cannot serve arbitrarily deep locations).
    fn locations(context: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(context
            .conn()
            .and_then(|mut c| svc::entity::locations(&mut c)))
    }

    /// One entity, or null when `id` matches none.
    fn entity(context: &GraphQLContext, id: ID) -> FieldResult<Option<Entity>> {
        gql(context
            .conn()
            .and_then(|mut c| svc::entity::get(&mut c, &id)))
    }

    /// Non-location entities with no parent (homeless items).
    fn root_items(context: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(context
            .conn()
            .and_then(|mut c| svc::entity::root_items(&mut c)))
    }

    /// Every tag.
    fn tags(context: &GraphQLContext) -> FieldResult<Vec<Tag>> {
        gql(context.conn().and_then(|mut c| svc::tag::list(&mut c)))
    }

    /// Case-insensitive substring match on entity names, clamped to 1..=200 rows.
    fn search(
        context: &GraphQLContext,
        query: String,
        #[graphql(default = DEFAULT_SEARCH_LIMIT)] limit: Option<i32>,
    ) -> FieldResult<Vec<Entity>> {
        let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        let limit = i64::from(limit.clamp(1, MAX_SEARCH_LIMIT));
        gql(context
            .conn()
            .and_then(|mut c| svc::entity::search(&mut c, &query, limit)))
    }
}
