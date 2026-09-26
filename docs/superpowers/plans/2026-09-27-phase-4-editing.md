# Phase 4: Editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Items and locations can be created, edited, moved, tagged and deleted from the UI; entity types and tags have management pages; the home-page statistics update after every change.

**Architecture:** Backend gains write services (`svc::entity::{create, update, delete, set_tags}`, `svc::entity_type::{create, update, delete}`, `svc::tag::{create, update, delete}`, `svc::attachment::{delete, set_primary}`) with the spec's validation rules enforced in `svc`, and a juniper `Mutation` root whose every resolver starts with `ctx.require_write()?`. Frontend gains one hook per mutation built on a shared `useRefetchingMutation` (refetches by operation name and evicts the moved entity's old and new parents from the Apollo cache), a shared `EntityForm` used for create and edit, a `LocationPicker`, a `TagPicker`, a `ConfirmDialog`, and pages for new/edit entities, types and tags. No new migrations.

**Tech Stack:** as phases 1–3. juniper `GraphQLInputObject` derives for inputs; diesel `AsChangeset` updates (`treat_none_as_null = true` is already on the models); React controlled forms (no form library), `Intl.NumberFormat`-free money parsing in a small util.

**Spec:** `docs/superpowers/specs/2026-09-25-home-tracker-design.md` (§3 rows 5, 6, 13, 21, 22; §6 Mutation side and validation rules; §9 attachment delete; §10; §11; §14; §15 "Phase 4").

## Global Constraints

- All phase-1–3 constraints hold (edition 2024, clippy `-D warnings`, fmt, OpenSSL-free, `site/build/index.html` before cargo, `yarn` only, semantic colour classes, hooks own GraphQL, `toast.error` for errors, no em dashes in docs, commit trailer `Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF`).
- Every mutation resolver's first statement is `ctx.require_write()?` (spec §10). A `Role::ReadOnly` actor gets `Forbidden: write access required` before any validation runs.
- Validation lives in `svc`, returns `anyhow` errors with user-readable messages, and is tested at the GraphQL seam (spec §6):
  - `name` is trimmed and must be non-empty (entities, types, tags).
  - `entityTypeId` must exist.
  - `parentId` (when present) must exist, must not be the entity itself or any descendant (cycle), and must be a location-type entity unless the child is not a location either (items may nest in items; a location may never be placed under an item).
  - `tagIds` must all exist; `set_tags` replaces the full set.
  - `assetId` is assigned on create as `max(asset_id) + 1` when the caller sends none (the input has no `assetId` field in v1; every create assigns one).
  - `quantity` must be ≥ 0; cents must be ≥ 0.
  - Deleting an entity with children is refused with a message naming the count (spec row 5).
  - Deleting an entity type still used by entities is refused with a message naming the count (spec §6). Changing a type from location to non-location is refused while any entity of that type has a location child.
  - Deleting a tag cascades its `tag_entities` links (FK) and is otherwise unconditional. A tag's `parentId` must exist and must not create a cycle.
  - `deleteAttachment` removes the row and its thumbnails (FK cascade) and deletes `originals/<sha256>` only when no other attachment row shares the hash (spec §9). `setPrimaryPhoto` clears `is_primary` on the entity's other photos and sets it on the given one, which must be a photo of that entity.
- Timestamps: `updated_at` is set to now on every update; `created_at`/`updated_at` set on create.
- Frontend refetch policy after any mutation: `GetLocations`, `GetSummary`, `GetRootItems`, `GetEntityTypes`, `GetTags`, plus `GetEntity` for the affected id and, on a move, `cache.evict` for the old and new parent ids followed by `cache.gc()`.
- Money in forms is entered in major units as a decimal string and stored as cents: `parseMoney("12.5") = 1250`, `parseMoney("1,234.56") = 123456`, `parseMoney("") = 0`, `parseMoney("abc")` is an error shown inline; `formatMoneyInput(1250) = "12.50"`.
- No `window.confirm`/`alert`: deletes use a `ConfirmDialog` component (role `dialog`, labelled, focus moves to the cancel button, Escape cancels).
- Carry-overs from the phase-3 review that this phase MUST close: sidebar highlights an item's parent location on `/items/:id`; search box seeded from `?q`; sidebar shows an error line instead of "No locations yet." on failure; `useEntity` toast message neutral; `HomePage` uses `useCurrency`; `useCurrency` reads the summary without toasting; `router.dispose()` on unmount; `fieldValue` has a `never` exhaustiveness check and a test that the TS `AttachmentKind`/`FieldKind` unions equal the backend enum names (pulled from an introspection query in a Rust test that writes nothing, or a hand-kept list checked by a Rust test against `AttachmentKind::ALL`); drawer closes on Escape and returns focus to the hamburger; thumbnail generation bounded by a `tokio::sync::Semaphore` of `available_parallelism()` and explicit `image::Limits` (max 64 MP, 256 MiB alloc); `/attachments` routes exempt from `CompressionLayer`.

## Review Focus

1. Moving a location under its own descendant through the API is refused, and the `LocationPicker` never offers the entity itself or its descendants. Task 2 test and Task 5 test.
2. A read-only actor calling any of the eleven mutations gets `Forbidden` and no row changes. Task 3 test (drives the schema directly with a `Role::ReadOnly` context and asserts row counts unchanged).
3. Creating an item with a name of only whitespace is refused server-side; the form disables submit until a non-blank name is typed. Task 2 and Task 5 tests.
4. `parseMoney` on `"12."`, `".5"`, `"1,2,3"`, `"-1"`, `"1e3"`: the first two parse (`1200`, `50`), the rest are errors. Task 5 test.
5. Deleting an item from its page navigates to the parent location (or home when parentless) and the location page no longer lists it without a reload (refetch + eviction). Task 6 test with `MockedProvider` refetch assertions.

---

### Task 1: Type and tag write services, compression exemption, thumbnail bounds

**Files:**
- Modify: `src/svc/entity_type.rs`, `src/svc/tag.rs`, `src/routes.rs`, `src/svc/thumbnail_service.rs`, `src/svc/thumbnail.rs`, `tests/attachments.rs`

**Interfaces:**
- Produces: `svc::entity_type::{create(conn, NewEntityType) -> Result<EntityType>, update(conn, id, EntityTypeChanges) -> Result<EntityType>, delete(conn, id) -> Result<()>}` with `pub struct NewEntityType { pub name: String, pub description: Option<String>, pub icon: Option<String>, pub is_location: bool }` and `EntityTypeChanges` = the same fields; `svc::tag::{create(conn, NewTag) -> Result<Tag>, update(conn, id, TagChanges) -> Result<Tag>, delete(conn, id) -> Result<()>}` with `NewTag { name, description, color, icon, parent_id }`.
- Produces: `svc::entity_type::location_child_count(conn, type_id) -> Result<i64>` (entities of this type that have at least one location-type child).
- Produces: `routes::app` applies `CompressionLayer` only to the GraphQL and static routers, never to `/attachments`; `ThumbnailService` gains `permits: Semaphore` and `generate_bytes` applies `image::Limits { max_image_width: 8192, max_image_height: 8192, max_alloc: 256 MiB }` via `ImageReader::limits`.

- [ ] **Step 1: Failing tests**

`svc::entity_type` tests: `create_trims_the_name_and_rejects_blank`, `update_refuses_location_to_item_while_location_children_exist` (Tote type holds Tote A whose child... in the sample Tote A has only items; add a location child under Tote A first, then `update(tote, is_location=false)` errors mentioning "location children"), `update_allows_location_to_item_when_no_location_children`, `delete_refuses_a_type_in_use` (Item → error naming 4 entities), `delete_removes_an_unused_type`, `delete_refuses_the_seeded_built_ins` (ids `LOCATION_TYPE_ID`/`ITEM_TYPE_ID` are never deletable).
`svc::tag` tests: `create_and_update_round_trip`, `parent_must_exist_and_not_cycle` (Tools.parent = Electronics whose parent is Tools → error), `delete_cascades_links` (delete Tools → Drill has no tags, `tag_entities` count drops).
`tests/attachments.rs`: `originals_are_not_compressed` (seed a `text/plain` attachment, GET with `Accept-Encoding: gzip` → no `content-encoding`, body is the raw bytes); `generation_is_bounded_by_the_semaphore` (assert `ThumbnailService::permits()` equals `available_parallelism`, and that a request still succeeds; a full contention test is not required); `svc::thumbnail` unit test `oversized_dimensions_are_rejected_by_limits` (a PNG header claiming 20000×20000 fails with a limits error, not an allocation).

- [ ] **Step 2: Implement**

Writes use `diesel::insert_into(...).values(&model).execute(conn)` with `id = Uuid::now_v7().to_string()` and `now = Utc::now().naive_utc()`, then `get` to return the row; updates use `diesel::update(table.find(id)).set((cols..., updated_at.eq(now)))`. Cycle check for tags: walk `parent_id` from the proposed parent with a visited set; if it reaches `id`, error. Compression: build `Router::new().merge(graphql_routes).route(assets).layer(Compression)` then `.merge(attachment_routes)` outside the layer (keep the `immutable_cache` ordering test green). Semaphore: `permits: Semaphore::new(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2))`, acquire before `spawn_blocking`.

- [ ] **Step 3: Verify, commit**

`cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`

```bash
git add src tests
git commit -m "feat: type and tag write services, bounded thumbnail generation, uncompressed originals

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 2: Entity and attachment write services

**Files:**
- Modify: `src/svc/entity.rs`, `src/svc/attachment.rs`, `src/svc/fixtures.rs` (only if a helper is needed)

**Interfaces:**
- Produces: `svc::entity::EntityInput { name, description, entity_type_id, parent_id, archived, quantity, insured, serial_number, model_number, manufacturer, notes, lifetime_warranty, warranty_expires: Option<NaiveDate>, warranty_details, purchase_date, purchase_from, purchase_price_cents: Cents, sold_date, sold_to, sold_price_cents: Cents, sold_notes, tag_ids: Option<Vec<String>> }` (all optional strings as `Option<String>`, booleans/f64 with defaults), `create(conn, EntityInput) -> Result<Entity>`, `update(conn, id, EntityInput) -> Result<Entity>`, `delete(conn, id) -> Result<()>`, `set_tags(conn, id, &[String]) -> Result<()>`, `next_asset_id(conn) -> Result<AssetId>`, `validate_parent(conn, child_id: Option<&str>, child_is_location: bool, parent_id: &str) -> Result<()>`.
- Produces: `svc::attachment::{delete(conn, data_dir, id) -> Result<()>, set_primary(conn, attachment_id) -> Result<Entity>}`.

- [ ] **Step 1: Failing tests** (all on `seed_sample`)

`create_assigns_the_next_asset_id` (max is 5 → new item gets 6; a second create gets 7); `create_trims_and_rejects_blank_names`; `create_rejects_unknown_type_and_parent`; `create_rejects_a_location_under_an_item` (type Location, parent Drill → error mentioning "location"); `create_allows_an_item_under_an_item` (Screws under Drill); `update_moves_and_replaces_tags` (move Screws to House, tags [Electronics] → tags exactly that); `update_refuses_a_cycle` (House.parent = Tote A → error mentioning "descendant"; Tote A.parent = Tote A → error); `update_rejects_negative_quantity_and_cents`; `delete_refuses_with_children` (Garage → error mentioning "2 children"); `delete_removes_an_item_and_its_tags_and_attachments` (Drill → gone; `tag_entities` for it gone; attachments gone via FK; thumbnails gone); `set_tags_rejects_unknown_ids`; `attachment_delete_removes_the_file_only_when_unshared` (write two rows sharing a sha to a temp `originals/`, delete one → file stays; delete the other → file gone); `set_primary_moves_the_flag_within_the_entity` and `set_primary_rejects_a_non_photo_or_foreign_attachment`.

- [ ] **Step 2: Implement**

`validate_parent`: load parent (`get` → error "parent not found"); `is_location(parent)`; if `!parent_is_location && child_is_location` → error "a location cannot be placed under an item"; if `!parent_is_location && !child_is_location` → allowed; when `child_id` is `Some`, refuse `parent_id == child_id` and refuse if `ancestors(parent_id)` contains `child_id` (error "cannot move an entity under its own descendant"). `create`: validate name/type/parent/tags, `next_asset_id`, insert, `set_tags` if given, return `get`. `update`: same validation with `child_id = Some(id)`, `diesel::update(...).set(&changes)` where changes is an `AsChangeset` struct with `updated_at`, then tags. `delete`: count children → error `"{name} still contains {n} entities; move them first"` when `n > 0`, else `diesel::delete(entities.find(id))`. Attachment delete: load row, delete it, count remaining rows with the same sha; if zero, `fs::remove_file(original_path(data_dir, &sha))` ignoring NotFound. `set_primary`: load attachment, require `kind == Photo`, `UPDATE attachments SET is_primary = (id = ?) WHERE entity_id = ? AND kind = 'photo'`, return the entity.

- [ ] **Step 3: Verify, commit**

```bash
git add src
git commit -m "feat: entity and attachment write services with the spec's validation rules

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 3: GraphQL `Mutation` root

**Files:**
- Create: `src/graphql/inputs.rs`, `src/graphql/mutation.rs`, `tests/graphql_mutations.rs`
- Modify: `src/graphql/mod.rs`, `src/graphql/schema.rs` (replace `EmptyMutation`), `src/graphql/context.rs` (a `data_dir: PathBuf` field so `deleteAttachment` can remove files; `GraphQLContext::new(pool, actor, data_dir)`), `src/api/graphql.rs`, `src/routes.rs`, `tests/graphql_queries.rs` (introspection test gains the mutation shapes)

**Interfaces:**
- Produces `#[derive(GraphQLInputObject)] EntityInput`, `EntityTypeInput`, `TagInput` exactly as spec §6 (`LocalDate` for dates, `Int` cents, `[ID!]` tagIds), with `impl TryFrom<EntityInput> for svc::entity::EntityInput` (negative cents → error).
- Produces the eleven mutations of spec §6 on `pub struct Mutation`; `Schema = RootNode<Query, Mutation, EmptySubscription<GraphQLContext>>`.

- [ ] **Step 1: Failing tests**

`tests/graphql_mutations.rs` (helper `mutate(server, doc, vars)` asserting no `errors`, and `mutate_err(...)` returning the first error message): `create_entity_returns_the_new_entity_with_an_asset_id`; `update_entity_moves_and_retags`; `delete_entity_refuses_with_children_then_succeeds_after_move`; `create_update_delete_entity_type` incl. the in-use refusal message; `create_update_delete_tag`; `delete_attachment_and_set_primary_photo`; `validation_errors_surface_as_graphql_errors` (blank name, cycle, location under item, unknown tag); `summary_updates_after_a_create` (value and item count change).
Read-only actor test (Review Focus 2) lives in `src/graphql/mutation.rs` `#[cfg(test)]`: build `GraphQLContext::new(pool, Actor::User { id, role: ReadOnly }, dir)` and `juniper::execute` each of the eleven mutations; every result has an error containing `Forbidden` and row counts are unchanged.

- [ ] **Step 2: Implement, verify, commit**

```bash
git add src tests
git commit -m "feat: GraphQL mutations for entities, types, tags and attachments behind the write gate

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 4: Frontend mutation hooks, refetch helper, and phase-3 carry-overs

**Files:**
- Create: `site/src/hooks/useRefetchingMutation.ts`, `site/src/hooks/useEntityTypes.ts`, `site/src/hooks/useTags.ts`, `site/src/hooks/useEntityMutations.ts` (`useCreateEntity`, `useUpdateEntity`, `useDeleteEntity`), `site/src/hooks/useEntityTypeMutations.ts`, `site/src/hooks/useTagMutations.ts`, `site/src/hooks/useAttachmentMutations.ts` (`useDeleteAttachment`, `useSetPrimaryPhoto`), `site/src/utils/money.ts`, tests for each
- Modify: `site/src/hooks/queries.ts` (documents `GET_ENTITY_TYPES`, `GET_TAGS`, and every mutation), `site/src/types/entity.ts` (`EntityInput`, `EntityTypeRef` gains `description`, `icon`, `entityCount`; `TagRef` gains `description`, `icon`, `parentId`, `entityCount`), `site/src/hooks/useCurrency.ts` (no toast), `site/src/hooks/useEntity.ts` (message `Error loading`), `site/src/page/HomePage.tsx` (`useCurrency`), `site/src/App.tsx` (`router.dispose()` on unmount), `site/src/components/Sidebar.tsx` (error line; highlight the parent location when on `/items/:id` using the entity's `parentId` from a small `useCurrentLocationId` hook that queries `GET_ENTITY` for the item id from the route), `site/src/components/SearchBox.tsx` (seed from `?q`), `site/src/page/PageTemplate.tsx` + `AppHeader.tsx` (Escape closes the drawer, focus returns to the hamburger), `site/src/page/ItemPage.tsx` (`never` check in `fieldValue`)

**Interfaces:**
- `useRefetchingMutation(doc, { refetch: string[]; evict?: (data) => string[] })` returns `[mutate, { loading, error }]`; on completion refetches the named active queries and evicts `Entity:<id>` cache entries for each returned id, then `gc()`.
- Hooks return `{ create(input) → Promise<EntityDetail> }` style functions plus `loading`; errors toast `Could not save`, `Could not delete`, with the server message appended.
- `utils/money.ts`: `parseMoney(input: string): number` (throws `MoneyParseError`), `formatMoneyInput(cents: number): string`.

- [ ] **Step 1: Failing tests** for `parseMoney`/`formatMoneyInput` (Review Focus 4 cases), each hook (`MockedProvider` with a mutation mock + refetch assertions via a second mock for `GetSummary` marked `newData`), `useCurrency` never toasts, `SearchBox` seeded from `?q`, drawer Escape, Sidebar error line, Sidebar highlights the item's parent, `fieldValue` union test (a compile-time `never` plus a runtime test that every `FieldKind`/`AttachmentKind` value renders).
- [ ] **Step 2: Implement, verify (`yarn test --run && yarn lint && yarn build`), commit**

```bash
git add site
git commit -m "feat(site): mutation hooks with refetch and eviction, money parsing, phase-3 carry-overs

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 5: `EntityForm`, `LocationPicker`, `TagPicker`, `ConfirmDialog`

**Files:**
- Create: `site/src/components/EntityForm.tsx`, `site/src/components/LocationPicker.tsx`, `site/src/components/TagPicker.tsx`, `site/src/components/ConfirmDialog.tsx`, `site/src/components/FormField.tsx` (label + input + inline error), tests for each

**Interfaces:**
- `<EntityForm mode="create"|"edit" initial?: EntityDetail defaultParentId?: string defaultTypeId?: string onSubmit(input: EntityInput) submitting />`: sections Basics (name, type select from `useEntityTypes`, parent via `LocationPicker`, quantity, description), Purchase (date, from, price), Warranty (lifetime checkbox, expires, details), Sold (date, to, price, notes), Details (manufacturer, model, serial, insured, notes, archived); Purchase/Warranty/Sold collapsed by default when the selected type is a location (spec row 2); submit disabled while name is blank or a money field fails to parse; inline errors; `Cancel` link back.
- `<LocationPicker value onChange excludeSubtreeOf?: string allowNone />`: a `<select>` of locations from `useLocations()` rendered with indentation by depth from `buildLocationTree`; excludes `excludeSubtreeOf` and its descendants (Review Focus 1); option `None (top level)`.
- `<TagPicker value: string[] onChange />`: checkboxes from `useTags()` plus an inline "New tag" input that calls `useCreateTag` and selects the result.
- `<ConfirmDialog open title body confirmLabel onConfirm onCancel />`: `role="dialog"`, `aria-labelledby`, focus on cancel at open, Escape cancels, backdrop click cancels.

- [ ] **Step 1: Failing tests** (Review Focus 1, 3, 4 among them: picker excludes subtree; submit disabled on blank; money errors inline; sections collapsed for a location type; edit mode prefills from `initial` including cents → "12.50"; `ConfirmDialog` focus and Escape).
- [ ] **Step 2: Implement, verify, commit**

```bash
git add site
git commit -m "feat(site): shared entity form with location and tag pickers and a confirm dialog

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 6: Pages and routes for editing, types and tags

**Files:**
- Create: `site/src/page/NewEntityPage.tsx` (`/locations/:id/new` and `/new`), `site/src/page/EditEntityPage.tsx` (`/items/:id/edit`, `/locations/:id/edit`), `site/src/page/EntityTypesPage.tsx` (`/types`), `site/src/page/TagsPage.tsx` (`/tags`), tests
- Modify: `site/src/App.tsx` (routes), `site/src/components/AppHeader.tsx` (nav links Types, Tags), `site/src/page/LocationPage.tsx` (enabled "Add item" → `/locations/:id/new`; "Add location" same route with `defaultTypeId` = Location via query param `?type=`; Edit and Delete buttons), `site/src/page/ItemPage.tsx` (Edit, Delete, per-attachment Delete and Make primary), `site/src/test/renderRoute.tsx` (new routes)

**Interfaces:**
- After create: navigate to the new entity's page. After delete: navigate to the parent location or `/`. After edit: back to the entity page.
- `EntityTypesPage`: table of types with counts; inline create form; edit in place; delete disabled with a title when `entityCount > 0`; the seeded built-ins are not deletable.
- `TagsPage`: same shape with colour swatch and parent select.

- [ ] **Step 1: Failing tests** (Review Focus 5 among them; redirects after create/delete asserted via `CurrentPath`; delete confirm flow; types page delete disabled when in use; tags page create).
- [ ] **Step 2: Implement, verify, commit**

```bash
git add site
git commit -m "feat(site): create, edit and delete flows plus types and tags pages

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 7: Acceptance and docs

- [ ] **Step 1:** Import the real backup into a scratch dir under `/home/.build/cargo-target/`, run the release binary on port 7043, and through `/graphql` (curl): create an item in Office, move it to Kitchen, tag it, edit its price, then delete it; after each step assert `summary` numbers change as expected and the location pages list it correctly. Headless-Chrome screenshots: the new-item form at `/locations/<office>/new` and the types page `/types`. Read the screenshots back. Kill the server, delete the scratch dir.
- [ ] **Step 2:** README (editing, types, tags), CLAUDE.md (mutation gate now real, refetch/eviction policy, `EntityForm` shared, `ConfirmDialog` rule), spec §11 route table (new routes) and §6 note if any shape drifted. Under 120 lines, no em dashes.
- [ ] **Step 3: Commit** `docs: editing flows, types and tags pages`

---

## Self-review

- **Spec coverage (Phase 4 exit criteria):** all §6 mutations (T3) on validated services (T1/T2); `EntityForm` shared for create/edit with parent and tag pickers (T5) used on the location page (create) and item page (edit) (T6); types page and tags page (T6); delete with the children rule (T2/T6); stats update (refetch policy T4, asserted in T3 `summary_updates_after_a_create` and T6). Carry-overs closed in T1 (compression, semaphore, limits) and T4 (UI polish items).
- **Type consistency:** `svc::entity::EntityInput` (T2) is what `graphql::inputs::EntityInput` converts into (T3); hook names in T4 are what T5/T6 import; `useRefetchingMutation` options are the same in T4's definition and T6's use.
- **Review Focus:** 1 → T2 `update_refuses_a_cycle` + T5 picker test; 2 → T3 read-only test; 3 → T2 blank-name + T5 submit-disabled tests; 4 → T4 `parseMoney` tests; 5 → T6 delete-navigation test.
