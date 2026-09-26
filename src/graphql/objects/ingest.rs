//! The AI ingest types: a batch, its items with the model's suggestion, and
//! their staged photos with what the model saw in each. The stored JSON is
//! read here, on resolve; a row whose JSON no longer parses shows no
//! suggestion (or description) rather than failing the whole batch query.

use chrono::{DateTime, NaiveDate, Utc};
use juniper::{FieldResult, ID};
use tracing::warn;

use super::saturating_int;
use crate::graphql::context::GraphQLContext;
use crate::graphql::schema::graphql_translate_anyhow as gql;
use crate::ingest::parse::{self, Confidence, ItemSuggestion, PhotoDescription};
use crate::kinds::{IngestBatchStatus, IngestItemStatus, IngestPhotoStatus, SuggestedKind};
use crate::models::{IngestBatch, IngestItem, IngestPhoto};
use crate::svc::attachment::DEFAULT_THUMB_URL_SIZE;
use crate::svc::ingest;

/// Photos grouped per item, on their way to becoming entities.
#[juniper::graphql_object(context = GraphQLContext, name = "IngestBatch")]
impl IngestBatch {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    /// The entity the batch was started from: the default parent of its items.
    fn parent_id(&self) -> Option<ID> {
        self.parent_id.as_deref().map(ID::new)
    }
    fn status(&self) -> IngestBatchStatus {
        self.status
    }
    /// In order. Positions have gaps, so label an item by its index here.
    fn items(&self, ctx: &GraphQLContext) -> FieldResult<Vec<IngestItem>> {
        gql(ctx.conn().and_then(|mut c| ingest::items(&mut c, &self.id)))
    }
    fn created_at(&self) -> DateTime<Utc> {
        self.created_at.and_utc()
    }
    /// The last activity; a batch idle for a week is deleted.
    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at.and_utc()
    }
}

/// One future entity: its photos, the model's suggestion once analysed, and
/// the entity it became once accepted.
#[juniper::graphql_object(context = GraphQLContext, name = "IngestItem")]
impl IngestItem {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    fn position(&self) -> i32 {
        self.position
    }
    fn status(&self) -> IngestItemStatus {
        self.status
    }
    /// Why the analysis failed, readable as-is.
    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    /// The prefilled record; null until analysed, or if the stored JSON no
    /// longer reads.
    fn suggestion(&self) -> Option<ItemSuggestion> {
        let raw = self.suggestion.as_deref()?;
        serde_json::from_str(raw)
            .inspect_err(|err| warn!(item = %self.id, "unreadable ingest suggestion: {err}"))
            .ok()
    }
    fn entity_id(&self) -> Option<ID> {
        self.entity_id.as_deref().map(ID::new)
    }
    /// In order; empty once the item is accepted or skipped.
    fn photos(&self, ctx: &GraphQLContext) -> FieldResult<Vec<IngestPhoto>> {
        gql(ctx
            .conn()
            .and_then(|mut c| ingest::photos(&mut c, &self.id)))
    }
}

/// What the model proposes for an item. Every field may be null: the photos
/// did not show it.
#[juniper::graphql_object(context = GraphQLContext, name = "IngestSuggestion")]
impl ItemSuggestion {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    fn manufacturer(&self) -> Option<&str> {
        self.manufacturer.as_deref()
    }
    fn model_number(&self) -> Option<&str> {
        self.model_number.as_deref()
    }
    fn serial_number(&self) -> Option<&str> {
        self.serial_number.as_deref()
    }
    fn quantity(&self) -> Option<f64> {
        self.quantity
    }
    fn purchase_date(&self) -> Option<NaiveDate> {
        self.purchase_date
    }
    fn purchase_from(&self) -> Option<&str> {
        self.purchase_from.as_deref()
    }
    /// Integer cents of the instance currency.
    fn purchase_price_cents(&self) -> Option<i32> {
        self.purchase_price_cents
            .map(|cents| saturating_int(cents.0))
    }
    fn warranty_expires(&self) -> Option<NaiveDate> {
        self.warranty_expires
    }
    fn lifetime_warranty(&self) -> Option<bool> {
        self.lifetime_warranty
    }
    fn warranty_details(&self) -> Option<&str> {
        self.warranty_details.as_deref()
    }
    fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }
    /// Names of existing tags, spelled as the tag is.
    fn tag_names(&self) -> &[String] {
        &self.tag_names
    }
    /// `low`, `medium` or `high`.
    fn confidence(&self) -> Option<&'static str> {
        self.confidence.map(|confidence| match confidence {
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
        })
    }
    fn reasoning(&self) -> Option<&str> {
        self.reasoning.as_deref()
    }
}

/// A staged photo. Its URLs are served exactly as an attachment's are.
#[juniper::graphql_object(context = GraphQLContext, name = "IngestPhoto")]
impl IngestPhoto {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    fn position(&self) -> i32 {
        self.position
    }
    fn status(&self) -> IngestPhotoStatus {
        self.status
    }
    /// Why the photo could not be described, readable as-is.
    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn mime_type(&self) -> &str {
        &self.mime_type
    }
    fn size_bytes(&self) -> i32 {
        saturating_int(self.size_bytes)
    }
    /// Where the original is served; the `?v=` tag changes with the bytes.
    fn url(&self) -> String {
        ingest::original_url(self)
    }
    /// A thumbnail of at most `size` pixels; null for non-images.
    fn thumbnail_url(
        &self,
        #[graphql(default = DEFAULT_THUMB_URL_SIZE)] size: i32,
    ) -> Option<String> {
        ingest::thumbnail_url(self, size)
    }
    /// What the model took the photo for, once described.
    fn suggested_kind(&self) -> Option<SuggestedKind> {
        self.suggested_kind
    }
    /// The model's one-line account of the photo.
    fn summary(&self) -> Option<String> {
        described(self).map(|description| description.summary)
    }
    /// The text visible in the photo, transcribed.
    fn text(&self) -> Option<String> {
        described(self).and_then(|description| description.text)
    }
}

/// `photo`'s stored description, if it has one that still reads.
fn described(photo: &IngestPhoto) -> Option<PhotoDescription> {
    let raw = photo.description.as_deref()?;
    parse::parse_description(raw)
        .inspect_err(|err| warn!(photo = %photo.id, "unreadable ingest description: {err}"))
        .ok()
}
