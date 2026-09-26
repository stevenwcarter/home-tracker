//! GraphQL input objects for the mutations, and their conversion into the
//! `svc` shapes. Validation beyond what the wire types can express lives in
//! `svc`; only what GraphQL cannot say (non-negative cents) is parsed here.

use anyhow::{Result, ensure};
use chrono::NaiveDate;
use juniper::{GraphQLInputObject, ID};

use crate::kinds::AttachmentKind;
use crate::money::Cents;
use crate::svc;

/// The quantity a create gets when the caller sends none.
const DEFAULT_QUANTITY: f64 = 1.0;

/// Every editable field of an entity. An update replaces them all, so an
/// omitted optional scalar resets to its default; an omitted `tagIds` leaves
/// the tags alone, while `[]` clears them.
#[derive(Debug, Clone, GraphQLInputObject)]
pub struct EntityInput {
    pub name: String,
    pub description: Option<String>,
    pub entity_type_id: ID,
    pub parent_id: Option<ID>,
    pub archived: Option<bool>,
    pub quantity: Option<f64>,
    pub insured: Option<bool>,
    pub serial_number: Option<String>,
    pub model_number: Option<String>,
    pub manufacturer: Option<String>,
    pub notes: Option<String>,
    pub lifetime_warranty: Option<bool>,
    pub warranty_expires: Option<NaiveDate>,
    pub warranty_details: Option<String>,
    pub purchase_date: Option<NaiveDate>,
    pub purchase_from: Option<String>,
    pub purchase_price_cents: Option<i32>,
    pub sold_date: Option<NaiveDate>,
    pub sold_to: Option<String>,
    pub sold_price_cents: Option<i32>,
    pub sold_notes: Option<String>,
    pub tag_ids: Option<Vec<ID>>,
}

/// A GraphQL `Int` money amount as [`Cents`]; absent is zero, negative is refused.
fn cents(value: Option<i32>, what: &str) -> Result<Cents> {
    let value = value.unwrap_or(0);
    ensure!(value >= 0, "{what} must not be negative");
    Ok(Cents(i64::from(value)))
}

impl TryFrom<EntityInput> for svc::entity::EntityInput {
    type Error = anyhow::Error;

    fn try_from(input: EntityInput) -> Result<Self> {
        Ok(Self {
            purchase_price_cents: cents(input.purchase_price_cents, "purchase price")?,
            sold_price_cents: cents(input.sold_price_cents, "sold price")?,
            name: input.name,
            description: input.description,
            entity_type_id: input.entity_type_id.into(),
            parent_id: input.parent_id.map(Into::into),
            archived: input.archived.unwrap_or_default(),
            quantity: input.quantity.unwrap_or(DEFAULT_QUANTITY),
            insured: input.insured.unwrap_or_default(),
            serial_number: input.serial_number,
            model_number: input.model_number,
            manufacturer: input.manufacturer,
            notes: input.notes,
            lifetime_warranty: input.lifetime_warranty.unwrap_or_default(),
            warranty_expires: input.warranty_expires,
            warranty_details: input.warranty_details,
            purchase_date: input.purchase_date,
            purchase_from: input.purchase_from,
            sold_date: input.sold_date,
            sold_to: input.sold_to,
            sold_notes: input.sold_notes,
            tag_ids: input
                .tag_ids
                .map(|ids| ids.into_iter().map(Into::into).collect()),
        })
    }
}

/// Every editable field of an entity type; an update replaces them all.
#[derive(Debug, Clone, GraphQLInputObject)]
pub struct EntityTypeInput {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub is_location: bool,
}

impl From<EntityTypeInput> for svc::entity_type::NewEntityType {
    fn from(input: EntityTypeInput) -> Self {
        Self {
            name: input.name,
            description: input.description,
            icon: input.icon,
            is_location: input.is_location,
        }
    }
}

/// Every editable field of a tag; an update replaces them all, so an omitted
/// `parentId` makes the tag top-level.
#[derive(Debug, Clone, GraphQLInputObject)]
pub struct TagInput {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub parent_id: Option<ID>,
}

impl From<TagInput> for svc::tag::NewTag {
    fn from(input: TagInput) -> Self {
        Self {
            name: input.name,
            description: input.description,
            color: input.color,
            icon: input.icon,
            parent_id: input.parent_id.map(Into::into),
        }
    }
}

/// Every AI setting the screen edits. `apiKey` omitted or null keeps the
/// stored key, `""` clears it, anything else replaces it. No `Debug`: it
/// carries the key.
#[derive(Clone, GraphQLInputObject)]
pub struct AiSettingsInput {
    pub base_url: String,
    pub vision_model: String,
    pub synthesis_model: String,
    pub extra_instructions: Option<String>,
    pub api_key: Option<String>,
}

impl From<AiSettingsInput> for svc::ai_settings::AiSettingsUpdate {
    fn from(input: AiSettingsInput) -> Self {
        Self {
            base_url: input.base_url,
            vision_model: input.vision_model,
            synthesis_model: input.synthesis_model,
            extra_instructions: input.extra_instructions,
            api_key: input.api_key,
        }
    }
}

/// The attachment kind the user chose for one staged photo on accept.
#[derive(Debug, Clone, GraphQLInputObject)]
pub struct IngestPhotoKindInput {
    pub photo_id: ID,
    pub kind: AttachmentKind,
}

impl From<IngestPhotoKindInput> for (String, AttachmentKind) {
    fn from(input: IngestPhotoKindInput) -> Self {
        (input.photo_id.into(), input.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> EntityInput {
        EntityInput {
            name: "Saw".to_owned(),
            description: None,
            entity_type_id: ID::new("t-item"),
            parent_id: Some(ID::new("e-garage")),
            archived: None,
            quantity: None,
            insured: None,
            serial_number: None,
            model_number: None,
            manufacturer: None,
            notes: None,
            lifetime_warranty: None,
            warranty_expires: None,
            warranty_details: None,
            purchase_date: None,
            purchase_from: None,
            purchase_price_cents: Some(1250),
            sold_date: None,
            sold_to: None,
            sold_price_cents: None,
            sold_notes: None,
            tag_ids: Some(vec![ID::new("t-tools")]),
        }
    }

    #[test]
    fn omitted_scalars_take_their_defaults() {
        let converted = svc::entity::EntityInput::try_from(input()).unwrap();
        assert_eq!(converted.entity_type_id, "t-item");
        assert_eq!(converted.parent_id.as_deref(), Some("e-garage"));
        assert_eq!(converted.quantity, DEFAULT_QUANTITY);
        assert!(!converted.archived && !converted.insured && !converted.lifetime_warranty);
        assert_eq!(converted.purchase_price_cents, Cents(1250));
        assert_eq!(converted.sold_price_cents, Cents(0));
        assert_eq!(converted.tag_ids, Some(vec!["t-tools".to_owned()]));
    }

    #[test]
    fn negative_cents_are_refused() {
        let err = svc::entity::EntityInput::try_from(EntityInput {
            purchase_price_cents: Some(-1),
            ..input()
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "purchase price must not be negative");
        let err = svc::entity::EntityInput::try_from(EntityInput {
            sold_price_cents: Some(-1),
            ..input()
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "sold price must not be negative");
    }
}
