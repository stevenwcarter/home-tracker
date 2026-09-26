use chrono::{DateTime, NaiveDate, Utc};
use juniper::{FieldResult, ID};

use crate::graphql::context::GraphQLContext;
use crate::graphql::schema::graphql_translate_anyhow as gql;
use crate::models::{Attachment, Entity, EntityField, EntityType, Tag};
use crate::svc;

/// A location or an item.
#[juniper::graphql_object(context = GraphQLContext, name = "Entity")]
impl Entity {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    fn entity_type(&self, ctx: &GraphQLContext) -> FieldResult<EntityType> {
        gql(ctx.conn().and_then(|mut c| {
            svc::entity_type::get(&mut c, &self.entity_type_id)?
                .ok_or_else(|| anyhow::anyhow!("entity type {} missing", self.entity_type_id))
        }))
    }
    /// Convenience for `entityType.isLocation`.
    fn is_location(&self, ctx: &GraphQLContext) -> FieldResult<bool> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity::is_location(&mut c, self)))
    }
    fn parent(&self, ctx: &GraphQLContext) -> FieldResult<Option<Entity>> {
        match &self.parent_id {
            None => Ok(None),
            Some(pid) => gql(ctx.conn().and_then(|mut c| svc::entity::get(&mut c, pid))),
        }
    }
    /// The parent's id, or null at the top of the hierarchy. Lets the client
    /// nest the flat `locations` query into a tree without a round trip per
    /// level (unlike `parent`, which loads the whole parent entity).
    fn parent_id(&self) -> Option<ID> {
        self.parent_id.as_deref().map(ID::new)
    }
    /// Root first, for breadcrumbs.
    fn ancestors(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity::ancestors(&mut c, &self.id)))
    }
    /// Direct children whose type is a location.
    fn child_locations(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity::child_locations(&mut c, &self.id)))
    }
    /// Direct children whose type is not a location.
    fn items(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity::items(&mut c, &self.id)))
    }
    fn archived(&self) -> bool {
        self.archived
    }
    /// `"000-007"`, or null when unassigned.
    fn asset_id(&self) -> Option<String> {
        self.asset_id.display_option()
    }
    fn quantity(&self) -> f64 {
        self.quantity
    }
    fn insured(&self) -> bool {
        self.insured
    }
    fn serial_number(&self) -> Option<&str> {
        self.serial_number.as_deref()
    }
    fn model_number(&self) -> Option<&str> {
        self.model_number.as_deref()
    }
    fn manufacturer(&self) -> Option<&str> {
        self.manufacturer.as_deref()
    }
    fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }
    fn lifetime_warranty(&self) -> bool {
        self.lifetime_warranty
    }
    fn warranty_expires(&self) -> Option<NaiveDate> {
        self.warranty_expires
    }
    fn warranty_details(&self) -> Option<&str> {
        self.warranty_details.as_deref()
    }
    fn purchase_date(&self) -> Option<NaiveDate> {
        self.purchase_date
    }
    fn purchase_from(&self) -> Option<&str> {
        self.purchase_from.as_deref()
    }
    fn purchase_price_cents(&self) -> i32 {
        self.purchase_price_cents.as_graphql_int()
    }
    fn sold_date(&self) -> Option<NaiveDate> {
        self.sold_date
    }
    fn sold_to(&self) -> Option<&str> {
        self.sold_to.as_deref()
    }
    fn sold_price_cents(&self) -> i32 {
        self.sold_price_cents.as_graphql_int()
    }
    fn sold_notes(&self) -> Option<&str> {
        self.sold_notes.as_deref()
    }
    fn tags(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Tag>> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::tag::for_entity(&mut c, &self.id)))
    }
    fn attachments(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Attachment>> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::attachment::for_entity(&mut c, &self.id)))
    }
    fn primary_photo(&self, ctx: &GraphQLContext) -> FieldResult<Option<Attachment>> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::attachment::primary_photo(&mut c, &self.id)))
    }
    fn fields(&self, ctx: &GraphQLContext) -> FieldResult<Vec<EntityField>> {
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity_field::for_entity(&mut c, &self.id)))
    }
    fn created_at(&self) -> DateTime<Utc> {
        self.created_at.and_utc()
    }
    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at.and_utc()
    }
}
