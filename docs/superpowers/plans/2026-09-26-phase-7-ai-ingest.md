# Phase 7: AI ingest workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From a location or item page, group photos per item, submit them, have the vision model describe each photo and the synthesis model build a prefilled item record, review each item with the existing form, and save it with its photos or skip it, with live progress over SSE.

**Architecture:** Server-staged batches (`ingest_batches`/`ingest_items`/`ingest_photos`) share the content-hash originals store and the sha-keyed thumbnails with attachments. A runner spawned on submit describes photos in parallel under a process-wide semaphore, synthesises per item, and publishes change events through a per-batch broadcast channel that an SSE route streams; the GraphQL `ingestBatch` query is the source of truth the client refetches on every event. Accept turns staged photos into attachments without moving bytes.

**Tech Stack:** Rust 2024, axum 0.8 (multipart, SSE), tokio (broadcast, semaphore), juniper 0.17, diesel/SQLite, base64, the Phase 6 `AiClient`; React 19 + Apollo 4 + `EventSource`, vitest.

**Spec:** `docs/superpowers/specs/2026-09-26-ai-ingest-design.md` (§2 decisions 8–19, §3, §4.3–4.5, §5, §6, §7, §8, §9, §10, §11, §12, §13, §14 Phase 7). Base spec: `docs/superpowers/specs/2026-09-25-home-tracker-design.md`.

## Global Constraints

- Rust edition 2024; clippy `-D warnings`, rustfmt clean; `cargo tree -i openssl` empty. New crate: `base64 = "0.22"` only.
- Status enums exactly as spec §4.3 via `kinds.rs` `text_enum!`: `IngestBatchStatus { Collecting, Processing, Reviewing, Done }`, `IngestItemStatus { Collecting, Queued, Analysing, Ready, Failed, Accepted, Skipped }`, `IngestPhotoStatus { Pending, Described, Failed }`, `SuggestedKind { Photo, Receipt, Warranty, Manual, Other }` (`Other` maps to `AttachmentKind::Attachment`).
- Sharer counting spans `attachments` and `ingest_photos`; the placing claim covers staging uploads; `insert_thumbnail` inserts only while a sharer exists (one `INSERT … SELECT … WHERE EXISTS`, same predicate as `sharing`).
- Model calls: one process-wide semaphore of 4; the image sent is the 1200 px WebP thumbnail as a `data:image/webp;base64,…` URL with `detail: auto`; caps `DESCRIBE_MAX_TOKENS = 6000`, `SYNTHESIS_MAX_TOKENS = 6000` (reasoning tokens count against them); the runner adds no retries of its own beyond the client's.
- Every mutation calls `ctx.require_write()` first; the SSE route is GET and read-only; `/graphql` stays POST-only.
- Staging upload: same limits, sniffing, statuses and JSON error shape as `/api/upload`, plus `409` when the item's batch is not `collecting`; response `201` with `{ id, position, status, title, mimeType, sizeBytes, url, thumbnailUrl }`.
- Photo URLs: `/ingest/photos/{id}?v=<sha prefix>` and `/ingest/photos/{id}/thumb/{size}?v=…`, served by the same code as attachments (ETag, immutable, nosniff, CSP sandbox).
- SSE: `GET /api/ingest/batches/{id}/events`, events named `photo`, `item`, `batch` with data `{"id":"…"}`, keep-alive comment every 15 s, exempt from compression, ends when the batch is `done` or deleted, 404 unknown.
- Frontend: semantic theme classes only; every GraphQL operation in a hook; `useUploadPhoto` generalised over a target, not duplicated; file inputs use `accept="image/*"` (HEIC never listed) and the camera button adds `capture="environment"`; copy from spec §9.
- Abandoned batches: deleted after 7 days without activity, at startup and every 24 h; interrupted `analysing` items are re-queued at startup.
- Never a real network call in tests; acceptance may make at most 8 real model calls (one batch of two items with two photos each), only with `OPENAI_API_KEY` present in the shell, never storing that key.
- CLAUDE.md under 120 lines, no em dashes. `homebox/`, `homebox-backup/`, `site/build/`, `data/` never `git add`ed; `site/build/index.html` exists; target dir `/home/.build/cargo-target`. Commits end with `Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF`.

## Review Focus

1. A photo whose bytes already belong to an attachment: staging it must not duplicate the file, deleting either side must keep the file while the other references it, and accepting the item must leave exactly one original. Task 1 and Task 2 tests.
2. A model answer that is not the JSON we asked for (prose, truncated, wrong types): the photo or item is marked failed with a readable message, nothing panics, the batch still reaches `reviewing`, and Retry works. Task 3 tests.
3. A server restart mid-batch: `analysing` items go back to `queued` and finish; a client that reconnects sees the current state (snapshot on open). Task 3 and Task 4 tests.
4. Accept with edited fields and changed kinds: the entity gets the user's values, not the suggestion; kinds are honoured; the primary photo is the first `photo`-kind image; the staged rows are gone and the files remain. Task 4 tests.
5. The collect screen on a phone: a batch with three items and eight photos, "Next item" and per-photo remove, a reload that lands back on the same batch with everything intact. Task 6 tests and Task 7 acceptance.

---

### Task 1: Ingest storage, sharing across tables, service functions

**Files:**
- Create: `migrations/2026-09-26-120000_ingest/{up,down}.sql`, `src/models/ingest.rs`, `src/svc/ingest.rs`
- Modify: `src/schema.rs` (regenerate), `src/kinds.rs`, `src/models/mod.rs`, `src/svc/mod.rs`, `src/svc/attachment.rs` (`sharing`, `insert_thumbnail`), `src/svc/fixtures.rs`, `CLAUDE.md`

**Interfaces:**
- Models: `IngestBatch { id, parent_id: Option<String>, status: IngestBatchStatus, created_at, updated_at }`, `IngestItem { id, batch_id, position: i32, status: IngestItemStatus, error: Option<String>, suggestion: Option<String>, entity_id: Option<String>, created_at, updated_at }`, `IngestPhoto { id, item_id, position: i32, sha256, mime_type, size_bytes: i64, title, status: IngestPhotoStatus, error: Option<String>, description: Option<String>, suggested_kind: Option<SuggestedKind>, created_at }`; `impl From<&IngestPhoto> for Blob`.
- `svc::ingest`: `create_batch(conn, parent_id: Option<&str>) -> Result<IngestBatch>` (parent must exist; one empty item at position 0); `get_batch`, `items(conn, batch_id)`, `photos(conn, item_id)`, `get_item`, `get_photo`, `open_batches(conn, parent_id: Option<&str>) -> Vec<IngestBatch>` (status != done, newest first); `add_item(conn, batch_id) -> IngestItem` (batch must be collecting); `remove_item(conn, data_dir, id)`; `remove_photo(conn, data_dir, id)` (batch must be collecting); `insert_photo_row(conn, item_id, sha256, mime, size, title) -> IngestPhoto` (next position; batch must be collecting, else `IngestError::NotCollecting`); `submit(conn, batch_id) -> Result<IngestBatch>` (deletes items with no photos; refuses a batch with no photos; items → queued; batch → processing); `mark_photo_described(conn, id, description: &serde_json::Value, kind: SuggestedKind)`, `mark_photo_failed(conn, id, error)`, `set_item_status(conn, id, status, error: Option<&str>)`, `set_item_suggestion(conn, id, &Value)`; `accept(conn, data_dir, item_id, entity::EntityInput, kinds: &[(String, AttachmentKind)]) -> Result<Entity>` (item must be ready or failed; transaction: create entity, one attachment per photo with the given or suggested kind, primary = first `photo` kind, delete the ingest photo rows, item accepted with `entity_id`, batch → done when no item is left open); `skip(conn, data_dir, item_id)`; `delete_batch(conn, data_dir, id)`; `settle_batch(conn, batch_id)` (processing → reviewing when every item is ready/failed/accepted/skipped; → done when every item is accepted/skipped); `cleanup_stale(conn, data_dir, cutoff: NaiveDateTime) -> Result<usize>`; `requeue_interrupted(conn) -> Result<Vec<String>>` (analysing → queued in processing batches; returns the batch ids); `touch(conn, batch_id)`; `IngestError { NotFound, NotCollecting, NotReviewable, Empty }` (thiserror) wrapped in anyhow with `downcast_ref` at the API layer.
- `svc::attachment::sharing` counts both tables; `insert_thumbnail` uses `INSERT INTO thumbnails … SELECT … WHERE EXISTS (attachments sha) OR EXISTS (ingest_photos sha)` via one shared `sharer_exists()` query fragment; `remove_original` unchanged in shape.
- Fixtures: `fixtures::ingest_batch(conn, parent) -> (batch, item)` and `fixtures::ingest_photo(conn, item_id, sha, mime)` for tests.

- [ ] **Step 1: Migration** from spec §4.3; `down.sql` drops the three tables. Regenerate the schema.
- [ ] **Step 2: Failing tests** (unit tests in `svc/ingest.rs` and `svc/attachment.rs` on `TestDb`): `create_batch_needs_an_existing_parent_and_starts_with_one_item`; `add_item_positions_increase_and_only_while_collecting`; `submit_drops_empty_items_and_refuses_an_empty_batch`; `submit_queues_items_and_starts_processing`; `accept_creates_the_entity_with_its_photos_kinds_and_primary` (Review Focus 4: two photos, kinds `[receipt, photo]` → primary is the second; the ingest photo rows are gone; the files remain; item accepted with entity_id); `accept_of_the_last_open_item_closes_the_batch`; `skip_removes_the_photos_files_unless_shared` (Review Focus 1: one sha shared with an attachment survives, the other is removed); `a_sha_shared_by_an_attachment_and_an_ingest_photo_survives_either_delete` (in `attachment.rs`); `insert_thumbnail_refuses_a_row_with_no_sharer` (delete the sharer, insert → no row); `cleanup_removes_only_stale_batches_and_their_files`; `requeue_interrupted_resets_analysing_items`; `settle_moves_processing_to_reviewing_and_to_done`.
- [ ] **Step 3: Implement; verify; commit** `feat: ingest batches, items and photos sharing the originals store`.

### Task 2: Staging upload and ingest photo routes

**Files:**
- Create: `src/api/ingest.rs` (upload + photo routes; Task 4 adds the SSE route here), `src/api/blob.rs` (shared serving), `tests/ingest_upload.rs`
- Modify: `src/api/upload.rs` (`receive`/`stage`/`UploadFailure`/`json_too_large` become `pub(crate)`), `src/api/attachments.rs` (delegate to `api::blob`), `src/svc/upload.rs` (`store_ingest_photo`), `src/api/mod.rs`, `src/routes.rs`

**Interfaces:**
- `svc::upload::store_ingest_photo(conn, data_dir, item_id, staged: Staged, filename: Option<&str>) -> Result<IngestPhoto, UploadError>`: same `place` → header check → 300 px thumbnail → commit path as `store`, inserting through `svc::ingest::insert_photo_row`; `UploadError` gains `Conflict` (batch not collecting → 409) and keeps `NotFound` for an unknown item.
- `api::blob`: `serve_original(data_dir, blob: &Blob, title: &str, id_for_logs: &str) -> Result<Response, AppError>` and `serve_thumb(thumbnails, blob, size: ThumbSize) -> Result<Response, AppError>` (the exact header set the attachment handlers use today); `api::attachments` and `api::ingest` both call them. `svc::ingest::{original_url(&IngestPhoto), thumbnail_url(&IngestPhoto, size)}` mirror the attachment helpers with the `/ingest/photos/{id}` prefix and `?v=`.
- `api::ingest::ingest_routes(pool, data_dir, thumbnails) -> Router` with `POST /api/ingest/items/{item_id}/photos` (body limit and default-limit override exactly as the upload router), `GET /ingest/photos/{id}`, `GET /ingest/photos/{id}/thumb/{size}`; mounted outside compression next to the upload and attachment routers.

- [ ] **Step 1: Failing tests** in `tests/ingest_upload.rs` (axum-test, `app_with_thumbnails`): `stages_a_jpeg_and_serves_it_at_all_sizes` (201 JSON shape, then original 200 with ETag/nosniff/CSP, thumbs 300/500/1200 → webp); `identical_bytes_shared_with_an_attachment_reuse_the_file` (Review Focus 1: originals count unchanged, thumbnails shared); `rejects_html_as_415_and_leaves_nothing`; `unknown_item_is_404`; `a_submitted_batch_refuses_more_photos_with_409`; `read_only_actor_is_403`; `over_limit_is_413_json`; `attachment_routes_still_behave_the_same` (the existing `tests/attachments.rs` suite is the pin; add one assertion here that the two handlers emit identical header sets for the same blob).
- [ ] **Step 2: Implement; verify; commit** `feat: staging uploads and ingest photo routes sharing the blob handlers`.

### Task 3: Runner, events hub, prompts wiring, restart and cleanup

**Files:**
- Create: `src/ingest/mod.rs`, `src/ingest/events.rs`, `src/ingest/runner.rs`, `src/ingest/parse.rs` (description/suggestion structs and lenient parsing), `tests/ingest_runner.rs`
- Modify: `src/ai/prompts.rs` (`synthesis_user_message`, `describe_user_message`), `src/lib.rs`, `src/graphql/context.rs` (`ingest: Arc<IngestRunner>`, `with_ingest`), `src/routes.rs` (construct and share the runner), `src/main.rs` (startup re-queue, cleanup schedule), `Cargo.toml` (`base64`)

**Interfaces:**
- `ingest::events::IngestEvents`: `new() -> Arc<Self>`, `subscribe(&self, batch_id) -> broadcast::Receiver<IngestEvent>` (creates the channel, capacity 64), `publish(&self, batch_id, IngestEvent)`, `close(&self, batch_id)` (drops the sender so streams end); `IngestEvent { kind: EventKind::{Photo, Item, Batch}, id: String }`.
- `ingest::parse`: `PhotoDescription { kind: SuggestedKind, summary: String, text: Option<String>, details: serde_json::Value }` and `ItemSuggestion` (spec §4.4 fields, all `Option`, `tag_names: Vec<String>`, `confidence: Option<String>`, `reasoning: Option<String>`) with `parse_description(&str) -> Result<PhotoDescription, ParseError>` and `parse_suggestion(&str) -> Result<ItemSuggestion, ParseError>` that tolerate a fenced ```json block and surrounding prose (extract the first `{…}` object), reject non-object JSON, coerce numeric strings for `quantity`/`purchase_price_cents`, drop invalid dates to `None`, and filter `tag_names` to a caller-supplied allow-list case-insensitively.
- `ingest::runner::IngestRunner { pool, data_dir, thumbnails: Arc<ThumbnailService>, ai: Arc<AiState>, events: Arc<IngestEvents>, permits: Arc<Semaphore> }`: `new(...) -> Arc<Self>`; `spawn_batch(self: &Arc<Self>, batch_id: String)` (tokio task; returns immediately); `spawn_item(self: &Arc<Self>, item_id: String)`; `run_batch(&self, batch_id) -> Result<()>` and `run_item(&self, item_id) -> Result<()>` (awaitable, used by tests); `describe_photo` (permit → thumbnail 1200 via `get_or_generate(&Blob::from(&photo), ThumbSize 1200)` → data URL → `ChatRequest` with `VISION_SYSTEM` + extra instructions, `json_schema` `photo_description_schema()`, `max_tokens DESCRIBE_MAX_TOKENS` → parse → mark described/failed → publish photo); `synthesise_item` (all described photos → `SYNTHESIS_SYSTEM` + user message from `prompts::synthesis_user_message(descriptions, currency, tag_names)` → parse → set suggestion, item ready/failed → publish item); after each item, `settle_batch` and publish batch; `close` the channel when done. `AiError::NotConfigured` at run time fails the item with "AI is not configured". DB work through `spawn_blocking`; no connection held across awaits.
- `GraphQLContext.ingest: Arc<IngestRunner>`; `GraphQLContext::new` builds a disabled runner (fresh `ThumbnailService`, `AiState::disabled()`, fresh events) and `with_ingest` replaces it; `routes()` builds one runner from the real thumbnails/AI state and passes it as an Extension to the GraphQL handler (Task 4 wires the mutations) and to the SSE route.
- `main.rs::serve`: after migrations, `svc::ingest::requeue_interrupted` then `runner.spawn_batch` for each id; `svc::ingest::cleanup_stale(now - 7 days)` at startup and from a `tokio::time::interval(24h)` task, logging counts.

- [ ] **Step 1: Failing tests** in `tests/ingest_runner.rs` with `FakeAiClient`: `describes_every_photo_in_parallel_and_synthesises_the_item` (two photos; fake answers pushed in order; assert two vision requests each with one image part whose URL starts `data:image/webp;base64,` and the system prompt; one synthesis request whose user text contains both summaries and the currency; item ready with the suggestion; batch reviewing; events published: 2 photo, 1 item, 1 batch); `a_failed_photo_does_not_stop_the_item` (Review Focus 2: fake returns prose for photo 1 → failed with "not valid JSON" message, photo 2 ok, item ready); `an_item_with_no_described_photo_is_failed`; `retry_reruns_only_failed_photos_and_the_synthesis`; `tag_names_are_filtered_to_existing_tags_case_insensitively`; `a_not_configured_client_fails_the_item_readably`; `parallelism_is_bounded_by_the_semaphore` (a fake that blocks until released; with 6 photos at most 4 in flight); `requeue_on_startup_finishes_an_interrupted_batch` (Review Focus 3: mark an item analysing by hand, call `requeue_interrupted` + `run_batch`, item ready). Unit tests in `ingest/parse.rs` for fenced JSON, prose around JSON, numeric strings, bad dates, non-object JSON.
- [ ] **Step 2: Implement; verify; commit** `feat: ingest runner with bounded parallel analysis, synthesis and events`.

### Task 4: GraphQL surface and SSE route

**Files:**
- Create: `src/graphql/objects/ingest.rs`, `tests/graphql_ingest.rs`, `tests/ingest_events.rs`
- Modify: `src/graphql/{query,mutation,inputs}.rs`, `src/graphql/objects/mod.rs`, `src/kinds.rs` (GraphQL enum derives for the new enums, as `AttachmentKind` has), `src/api/ingest.rs` (SSE route), `src/routes.rs`, `tests/graphql_queries.rs` (schema pins)

**Interfaces:** exactly spec §6 (`IngestBatch`, `IngestItem`, `IngestSuggestion`, `IngestPhoto` with `url`/`thumbnailUrl(size)`/`summary`/`text` from the stored description, `IngestPhotoKindInput`, `Query.ingestBatch`, `Query.openIngestBatches(parentId)`, the ten mutations). `submitIngestBatch` calls `ctx.ingest.spawn_batch` after the transaction commits; `retryIngestItem` resets the item to queued and calls `spawn_item`; `acceptIngestItem` converts `EntityInput` with the existing `TryFrom`; `deleteIngestBatch` closes the event channel. SSE handler: `Sse<impl Stream>` from `BroadcastStream` mapping `IngestEvent` to `Event::default().event(kind).json_data({id})`, `KeepAlive::new().interval(15 s)`; subscribe first, then load the batch (404 if missing; if already done, emit one `batch` event and end).

- [ ] **Step 1: Failing tests.** `tests/graphql_ingest.rs` through `/graphql` with `app_with_ai` + fake client: `create_add_upload_submit_review_accept_flow` (the whole happy path: create with a parent → add item → stage two photos via the HTTP upload → submit → wait for `ingestBatch.status == REVIEWING` (poll the query with a bounded loop, since the fake answers immediately) → accept with edited `name` and kinds → the entity exists with the user's name, two attachments, primary right; batch DONE; Review Focus 4); `skip_then_done`; `retry_after_a_failed_item`; `open_batches_lists_only_unfinished_ones_for_the_parent`; `mutations_require_write` (each of the ten with the read-only app); `submit_of_an_empty_batch_is_refused`; `accept_of_a_queued_item_is_refused`; schema pins for the four enums and the `IngestSuggestion` field types. `tests/ingest_events.rs`: `events_stream_photo_item_and_batch_and_ends_on_done` (open the stream with axum-test or a raw `oneshot` and read the body incrementally; publish through the runner by submitting a batch; assert the three event names in order and stream end after accept), `unknown_batch_is_404`, `a_done_batch_yields_one_batch_event_then_ends`, `stream_is_not_compressed` (Accept-Encoding gzip → no Content-Encoding).
- [ ] **Step 2: Implement; verify; commit** `feat: ingest GraphQL surface and SSE progress stream`.

### Task 5: Frontend types and hooks

**Files:**
- Create: `site/src/types/ingest.ts`, `site/src/hooks/useIngestBatch.ts`, `site/src/hooks/useIngestMutations.ts`, `site/src/hooks/useOpenIngestBatches.ts`, `site/src/utils/ingest.ts`, tests for each hook and util
- Modify: `site/src/hooks/queries.ts`, `site/src/hooks/useUploadPhoto.ts` (+ test), `site/src/components/PhotoUploader.tsx` (`target` prop), `site/src/utils/uploadErrors.ts` (409 message "This batch is no longer collecting photos")

**Interfaces:**
- `types/ingest.ts`: `IngestBatchStatus`, `IngestItemStatus`, `IngestPhotoStatus`, `SuggestedKind` unions (uppercase as GraphQL spells them, tied to `src/kinds.rs` by the existing enum-parity test pattern), `IngestBatch`, `IngestItem`, `IngestPhoto`, `IngestSuggestion`, `IngestPhotoKindInput`.
- `useUploadPhoto(target: UploadTarget)` where `UploadTarget = { kind: 'entity'; entity: { id: string; parentId: string | null } } | { kind: 'ingestItem'; itemId: string }`; the URL is `/api/upload/{id}` or `/api/ingest/items/{id}/photos`; refetch policy `REFETCH_AFTER_UPLOAD` for entities, `['GetIngestBatch']` for ingest items; `primary` only meaningful for entities. `PhotoUploader` takes `target` and the same children; existing call sites pass `{ kind: 'entity', entity }`.
- `useIngestBatch(id)` → `{ batch, loading, error, refetch }`: `useQuery(GET_INGEST_BATCH)` plus an `EventSource` on `/api/ingest/batches/{id}/events` opened in an effect (closed on unmount or id change), listening to `photo`, `item`, `batch` and `open`, each scheduling one `refetch` debounced by 150 ms; `onerror` is left to the browser's reconnect.
- `useIngestMutations()` → `{ createBatch(parentId), addItem(batchId), removeItem(id), removePhoto(id), submit(batchId), retryItem(id), acceptItem(id, input, kinds), skipItem(id), deleteBatch(id) }`, all through `useRefetchingMutation` with `['GetIngestBatch', 'GetOpenIngestBatches']`; `acceptItem` also uses `REFETCH_AFTER_WRITE` + `GetEntity` and evicts the created entity's parent (the input's `parentId`).
- `useOpenIngestBatches(parentId)` → `{ batches, loading }`.
- `utils/ingest.ts`: `suggestionToInput(suggestion, { typeId, parentId, tags }) -> EntityInput` (maps `tagNames` → ids case-insensitively; money and dates pass through; `quantity` default 1), `kindOfPhoto(photo) -> AttachmentKind` (suggested kind → attachment kind, `OTHER` → `ATTACHMENT`, missing → `PHOTO`), `nextReviewable(batch) -> IngestItem | null` (first item in `READY` or `FAILED`), `batchCounts(batch)`.

- [ ] **Step 1: Failing tests:** `useUploadPhoto` posts to the ingest URL for an ingest target and refetches `GetIngestBatch` (spied), still posts to the upload URL for entities (existing tests pass unchanged); `useIngestBatch` refetches on each event and on open (mock `EventSource` on `globalThis` recording listeners; dispatch `open`, `photo`, `item`; assert refetch count with the debounce collapsing a burst) and closes on unmount; `useIngestMutations.acceptItem` sends the input and kinds and evicts the parent; `suggestionToInput` maps every field and filters unknown tags; `nextReviewable` order; 409 message.
- [ ] **Step 2: Implement; verify (`yarn test --run && yarn lint && yarn build`); commit** `feat(site): ingest hooks, generalised uploads and suggestion mapping`.

### Task 6: Frontend pages and entry points

**Files:**
- Create: `site/src/page/IngestPage.tsx`, `site/src/components/ingest/{AiEntry,IngestCollect,IngestItemGroup,IngestProgress,IngestReview,IngestDone}.tsx`, tests for each
- Modify: `site/src/App.tsx` (`ingest/:batchId`), `site/src/page/LocationPage.tsx`, `site/src/page/ItemPage.tsx`, `site/src/components/EntityForm.tsx` (only if a prop is needed to accept an initial `EntityInput` in create mode; prefer a new optional `initialInput` prop), `CLAUDE.md`

**Interfaces:**
- `AiEntry({ parentId })`: when `useAiSettings().settings.hasApiKey` → button "Add item(s) with AI" (creates a batch, navigates to `/ingest/:id`), else a muted link "Set up AI" to `/settings/ai`; below it "Resume batch" links from `useOpenIngestBatches(parentId)` with the item count and age.
- `IngestPage`: `useIngestBatch(batchId)`; switches on status: `COLLECTING` → `IngestCollect`, `PROCESSING`/`REVIEWING` → `IngestProgress` + `IngestReview`, `DONE` → `IngestDone`; unknown batch → `NotFound`; heading "Add items with AI" with a breadcrumb to the parent.
- `IngestCollect`: item groups in order (`IngestItemGroup`: "Item N", thumbs 300 with a remove button each, `PhotoUploader` with two triggers "Add photos" (multiple) and "Take photo" (`capture="environment"`), "Remove item" when more than one item); "Next item"; "Submit for analysis" enabled when any photo exists; "Discard batch" through `ConfirmDialog`.
- `IngestProgress`: one chip per item showing status (`Queued`, `Analysing`, `Ready`, `Failed`, `Saved`, `Skipped`) with `aria-live="polite"`.
- `IngestReview`: for `nextReviewable(batch)`: the photo strip (thumb 300, a kind `<select>` per photo defaulting to `kindOfPhoto`, a `<details>` "What the AI saw" with summary and text), the confidence/reasoning line, `EntityForm` in create mode seeded with `suggestionToInput(...)` (keyed by item id), "Save item" (accept with the form's input and the kinds) and "Skip"; a failed item shows the error with "Retry" and "Skip"; nothing ready yet → "Analysing your photos…".
- `IngestDone`: counts, links to saved entities, "Add more items" (new batch with the same parent) and "Back to <parent name>".

- [ ] **Step 1: Failing tests.** `AiEntry` shows the button with a key and the link without, and resume links; `IngestCollect` renders groups, "Next item" adds one, remove photo calls `removePhoto`, "Submit for analysis" disabled with no photos and calls `submit`, the camera input carries `capture="environment"` and both inputs `accept="image/*"` without HEIC (Review Focus 5); `IngestReview` prefills the form from the suggestion, changing a kind and the name then Save calls `acceptItem` with the edited input and kinds, Skip calls `skipItem`, a failed item shows Retry; `IngestProgress` chips; `IngestDone` links; `IngestPage` switches on status and shows NotFound for an unknown batch; `LocationPage`/`ItemPage` render `AiEntry`; phone width: the collect grid wraps (class assertions) and the page has no fixed widths.
- [ ] **Step 2: Implement; verify; commit** `feat(site): AI ingest collect, review and done screens`.

### Task 7: Acceptance and docs

- [ ] **Step 1:** Build the real bundle and release binary; scratch dir under `/home/.build/cargo-target/`; import the real backup; serve on 7055 with the real `OPENAI_API_KEY` from the shell (never write it into the scratch DB or the log; `OPENAI_BASE_URL` unset). Through the UI in headless Chrome where practical, otherwise through GraphQL and curl: create a batch under the "Office" location, two items with two backup JPEGs each, submit, wait for `REVIEWING` (at most 8 real calls; record the models, latencies and token usage from the log), screenshot the review screen at desktop and phone width, accept one item with an edited name and skip the other, confirm the location page lists the new item with its primary photo and that `originals/` did not grow for photos already in the backup. Stop the server, delete the scratch dir.
- [ ] **Step 2:** README ("Adding items with AI" walkthrough, what the model sees, limits, HEIC note, the 7-day cleanup, restart behaviour), CLAUDE.md (`ingest/` module, `IngestRunner` on the context, SSE route, sharing across tables; under 120 lines, no em dashes), spec §5–§9 aligned with the implementation.
- [ ] **Step 3: Commit** `docs: AI ingest workflow`.

---

## Self-review

- **Spec coverage (Phase 7 exit, spec §14):** tables and services (T1), staging upload and photo routes (T2), runner with semaphore, events, restart re-queue and cleanup (T3), GraphQL and SSE (T4), hooks (T5), collect/review/done pages and entry points (T6), acceptance with the real model and docs (T7).
- **Type consistency:** `IngestBatch`/`IngestItem`/`IngestPhoto` models, `svc::ingest` function names, `IngestRunner::{spawn_batch, spawn_item, run_batch, run_item}`, `IngestEvents::{subscribe, publish, close}`, `UploadTarget`, `suggestionToInput`, `nextReviewable`, and the GraphQL names of spec §6 are spelled identically across tasks.
- **Review Focus:** 1 → T1 shared-sha tests + T2 reuse test; 2 → T3 prose/failed-photo tests; 3 → T3 requeue test + T4 done-batch stream test; 4 → T1 accept test + T4 flow test; 5 → T6 input attribute tests + T7 phone screenshot.
