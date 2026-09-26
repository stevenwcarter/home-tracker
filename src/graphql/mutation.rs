//! The GraphQL `Mutation` root. Every resolver passes the write gate first,
//! so a read-only actor is refused before any input is parsed or validated.

use juniper::{FieldResult, ID};

use super::context::GraphQLContext;
use super::inputs::{AiSettingsInput, EntityInput, EntityTypeInput, TagInput};
use super::schema::graphql_translate_anyhow as gql;
use crate::models::{Entity, EntityType, Tag};
use crate::svc;
use crate::svc::ai_settings::AiSettingsView;

pub struct Mutation;

#[juniper::graphql_object(context = GraphQLContext)]
impl Mutation {
    /// Creates an entity with the next asset id.
    fn create_entity(ctx: &GraphQLContext, input: EntityInput) -> FieldResult<Entity> {
        ctx.require_write()?;
        gql(svc::entity::EntityInput::try_from(input)
            .and_then(|input| svc::entity::create(&mut *ctx.conn()?, input)))
    }

    /// Replaces every editable field of entity `id`; moves and retags it.
    fn update_entity(ctx: &GraphQLContext, id: ID, input: EntityInput) -> FieldResult<Entity> {
        ctx.require_write()?;
        gql(svc::entity::EntityInput::try_from(input)
            .and_then(|input| svc::entity::update(&mut *ctx.conn()?, &id, input)))
    }

    /// Deletes entity `id` with its attachments; refused while it contains entities.
    fn delete_entity(ctx: &GraphQLContext, id: ID) -> FieldResult<bool> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity::delete(&mut c, &ctx.data_dir, &id))
            .map(|()| true))
    }

    /// Creates an entity type; the name is trimmed and must not be blank.
    fn create_entity_type(ctx: &GraphQLContext, input: EntityTypeInput) -> FieldResult<EntityType> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity_type::create(&mut c, input.into())))
    }

    /// Replaces every editable field of entity type `id`.
    fn update_entity_type(
        ctx: &GraphQLContext,
        id: ID,
        input: EntityTypeInput,
    ) -> FieldResult<EntityType> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity_type::update(&mut c, &id, input.into())))
    }

    /// Deletes entity type `id`; refused for built-in types and types in use.
    fn delete_entity_type(ctx: &GraphQLContext, id: ID) -> FieldResult<bool> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::entity_type::delete(&mut c, &id))
            .map(|()| true))
    }

    /// Creates a tag, optionally nested under `parentId`.
    fn create_tag(ctx: &GraphQLContext, input: TagInput) -> FieldResult<Tag> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::tag::create(&mut c, input.into())))
    }

    /// Replaces every editable field of tag `id`; an omitted parent makes it top-level.
    fn update_tag(ctx: &GraphQLContext, id: ID, input: TagInput) -> FieldResult<Tag> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::tag::update(&mut c, &id, input.into())))
    }

    /// Deletes tag `id`, unlinking it from its entities.
    fn delete_tag(ctx: &GraphQLContext, id: ID) -> FieldResult<bool> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::tag::delete(&mut c, &id))
            .map(|()| true))
    }

    /// Deletes attachment `id`, and its original once no attachment shares it.
    fn delete_attachment(ctx: &GraphQLContext, id: ID) -> FieldResult<bool> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::attachment::delete(&mut c, &ctx.data_dir, &id))
            .map(|()| true))
    }

    /// Makes photo `attachment_id` its entity's primary photo; returns the entity.
    fn set_primary_photo(ctx: &GraphQLContext, attachment_id: ID) -> FieldResult<Entity> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::attachment::set_primary(&mut c, &attachment_id)))
    }

    /// Saves the AI settings; a field set from the environment is refused,
    /// naming the variable.
    fn update_ai_settings(
        ctx: &GraphQLContext,
        input: AiSettingsInput,
    ) -> FieldResult<AiSettingsView> {
        ctx.require_write()?;
        gql(ctx
            .conn()
            .and_then(|mut c| svc::ai_settings::update(&mut c, &ctx.ai.env, input.into())))
    }
}

#[cfg(test)]
mod tests {
    use diesel::prelude::*;
    use juniper::Variables;

    use super::*;
    use crate::db::{ITEM_TYPE_ID, TestDb};
    use crate::graphql::context::{Actor, Role};
    use crate::graphql::schema::create_schema;
    use crate::models::{Attachment, TagEntity};
    use crate::schema::{attachments, entities, entity_types, settings, tag_entities, tags};
    use crate::svc::entity_type::NewEntityType;
    use crate::svc::fixtures::{SampleIds, seed_sample};

    /// Every row a mutation could touch, in a stable order.
    #[derive(Debug, PartialEq)]
    struct Snapshot {
        entities: Vec<Entity>,
        entity_types: Vec<EntityType>,
        tags: Vec<Tag>,
        attachments: Vec<Attachment>,
        tag_entities: Vec<TagEntity>,
        settings: Vec<(String, String)>,
    }

    fn snapshot(conn: &mut SqliteConnection) -> Snapshot {
        Snapshot {
            entities: entities::table
                .order(entities::id)
                .select(Entity::as_select())
                .load(conn)
                .unwrap(),
            entity_types: entity_types::table
                .order(entity_types::id)
                .select(EntityType::as_select())
                .load(conn)
                .unwrap(),
            tags: tags::table
                .order(tags::id)
                .select(Tag::as_select())
                .load(conn)
                .unwrap(),
            attachments: attachments::table
                .order(attachments::id)
                .select(Attachment::as_select())
                .load(conn)
                .unwrap(),
            tag_entities: tag_entities::table
                .order((tag_entities::tag_id, tag_entities::entity_id))
                .load(conn)
                .unwrap(),
            settings: settings::table.order(settings::key).load(conn).unwrap(),
        }
    }

    /// One document per mutation, each valid for a write actor on the sample
    /// (plus the unused type `spare_type`), so a refusal can only be the gate.
    fn mutations(ids: &SampleIds, spare_type: &str) -> [String; 12] {
        let item = ITEM_TYPE_ID;
        [
            format!(
                r#"mutation {{ createEntity(input: {{ name: "Saw", entityTypeId: "{item}" }}) {{ id }} }}"#
            ),
            format!(
                r#"mutation {{ updateEntity(id: "{}", input: {{ name: "Renamed", entityTypeId: "{item}" }}) {{ id }} }}"#,
                ids.loose
            ),
            format!(r#"mutation {{ deleteEntity(id: "{}") }}"#, ids.broken_lamp),
            r#"mutation { createEntityType(input: { name: "Bin", isLocation: true }) { id } }"#
                .to_owned(),
            format!(
                r#"mutation {{ updateEntityType(id: "{}", input: {{ name: "Crate", isLocation: true }}) {{ id }} }}"#,
                ids.tote_type
            ),
            format!(r#"mutation {{ deleteEntityType(id: "{spare_type}") }}"#),
            r#"mutation { createTag(input: { name: "Garden" }) { id } }"#.to_owned(),
            format!(
                r#"mutation {{ updateTag(id: "{}", input: {{ name: "Gadgets" }}) {{ id }} }}"#,
                ids.electronics
            ),
            format!(r#"mutation {{ deleteTag(id: "{}") }}"#, ids.tools),
            format!(r#"mutation {{ deleteAttachment(id: "{}") }}"#, ids.manual),
            format!(
                r#"mutation {{ setPrimaryPhoto(attachmentId: "{}") {{ id }} }}"#,
                ids.photo
            ),
            r#"mutation { updateAiSettings(input: { baseUrl: "https://llm.example/v1", visionModel: "v", synthesisModel: "s", apiKey: "k" }) { hasApiKey } }"#
                .to_owned(),
        ]
    }

    /// Runs `doc` against `ctx` and returns its field error messages.
    async fn run(doc: &str, ctx: &GraphQLContext) -> Vec<String> {
        let schema = create_schema();
        let (_, errors) = juniper::execute(doc, None, &schema, &Variables::new(), ctx)
            .await
            .unwrap_or_else(|e| panic!("{doc} did not execute: {e:?}"));
        errors
            .iter()
            .map(|e| e.error().message().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn a_read_only_actor_is_refused_every_mutation_and_nothing_changes() {
        let db = TestDb::new();
        let data = tempfile::tempdir().unwrap();
        let mut conn = db.pool.get().unwrap();
        let ids = seed_sample(&mut conn);
        let spare = svc::entity_type::create(
            &mut conn,
            NewEntityType {
                name: "Spare".to_owned(),
                description: None,
                icon: None,
                is_location: false,
            },
        )
        .unwrap();
        let before = snapshot(&mut conn);
        let docs = mutations(&ids, &spare.id);

        let reader = GraphQLContext::new(
            db.pool.clone(),
            Actor::User {
                id: "u".to_owned(),
                role: Role::ReadOnly,
            },
            data.path(),
        );
        for doc in &docs {
            let errors = run(doc, &reader).await;
            assert!(
                errors.len() == 1 && errors[0].contains("Forbidden"),
                "{doc}: {errors:?}"
            );
        }
        assert_eq!(snapshot(&mut conn), before);

        // The same documents succeed for a writer, so the refusals above came
        // from the gate and not from bad input.
        let writer = GraphQLContext::new(db.pool.clone(), Actor::Anonymous, data.path());
        for doc in &docs {
            let errors = run(doc, &writer).await;
            assert!(errors.is_empty(), "{doc}: {errors:?}");
        }
        assert_ne!(snapshot(&mut conn), before);
    }
}
