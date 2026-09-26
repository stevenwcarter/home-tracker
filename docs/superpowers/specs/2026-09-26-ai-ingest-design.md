# AI ingest design

Adds a guided "Add item(s) with AI" workflow to home-tracker: photos of an
item (overview, model/serial tag, receipt, warranty card) are analysed by an
OpenAI-compatible vision model, the findings are synthesised into a prefilled
item record, and the user reviews each item before it is saved. A settings
screen with an AI tab holds the endpoint, key and models.

This spec extends `2026-09-25-home-tracker-design.md` (the base spec). Where
this document is silent, the base spec applies. Section numbers here are
independent of the base spec's.

## 1. Purpose and scope

- Capture many items quickly from a phone or desktop: group photos per item,
  submit, review prefilled records one by one, save or skip.
- Work against any endpoint that speaks the OpenAI Chat Completions API with
  image inputs (OpenAI, OpenRouter, LiteLLM, Ollama, LM Studio).
- Keep the API key write-only: once saved it is never returned by any API.

Out of scope for this feature: receipts that list several items, editing the
prompts, custom (user-defined) fields, HEIC decoding on the server, cost
accounting, and running analysis for photos already attached to an entity.

## 2. Decisions

| # | Decision | Choice |
|---|---|---|
| 1 | Where the AI calls run | Server only. The browser never sees the key or the endpoint. |
| 2 | API style | Chat Completions (`POST {base_url}/chat/completions`) with `image_url` content parts. Synthesis asks for `response_format: {type: "json_schema", strict}`; if the endpoint rejects `json_schema` (HTTP 400 whose body mentions `response_format`), retry once with `{type: "json_object"}` and validate the JSON ourselves. |
| 3 | Key storage | Plaintext in the `settings` table. The SQLite file is treated as secret (documented). Encryption at rest can follow later without a schema change. |
| 4 | Environment overrides | `OPENAI_API_KEY` and `OPENAI_BASE_URL` win over the stored values when set. The AI tab shows those fields as "set from environment" and disabled. |
| 5 | Models | Two settings: `vision_model` (per-photo step) and `synthesis_model` (combine step), both default `gpt-5-mini`. Free-text fields. |
| 6 | Extra instructions | One optional free-text setting appended to both system prompts. |
| 7 | Test connection | A mutation that sends a tiny text-only chat completion to the synthesis model and reports ok/failure with latency. |
| 8 | Batch staging | Server-staged: photos are uploaded immediately into ingest tables backed by the same content-hash originals store. A page reload resumes the batch. |
| 9 | Progress transport | Server-Sent Events as a change signal; the GraphQL `ingestBatch` query is the source of truth and is refetched (debounced) on every event and on (re)connect. No polling. |
| 10 | Image sent to the model | The 1200 px WebP rendition as a base64 `data:` URL with `detail: "auto"`. Never the original bytes. |
| 11 | Concurrency | One process-wide semaphore of 4 model calls. Items in a batch run in parallel; photos within an item run in parallel; synthesis waits for the item's photos. Per call: 60 s timeout, two retries with backoff (1 s, 4 s) on 429 and 5xx, none on 4xx. |
| 12 | Fields the AI may fill | `name`, `description`, `manufacturer`, `model_number`, `serial_number`, `quantity`, `purchase_date`, `purchase_from`, `purchase_price_cents`, `warranty_expires`, `lifetime_warranty`, `warranty_details`, `notes`, plus `tag_names` restricted to existing tag names. |
| 13 | Photo kinds | Each photo is classified `photo`, `receipt`, `warranty`, `manual` or `other` (stored as `attachment` on accept). The first `photo`-kind image becomes the primary photo. The user can change kinds in review. |
| 14 | Review form | The existing `EntityForm` prefilled from the suggestion, type defaulting to Item, parent defaulting to the entity the batch was started from. |
| 15 | Failure | A failed photo is marked failed and synthesis uses the others; an item with no described photo is failed. A per-item retry re-runs its failed photos and the synthesis. |
| 16 | HEIC | Not decoded. Phone browsers transcode to JPEG for a file input whose `accept` is `image/*` (HEIC not listed) and the camera capture path yields JPEG. Desktop uploads of HEIC files get the existing 415. Documented limitation. |
| 17 | Abandoned batches | Deleted after 7 days without activity, at startup and every 24 h. |
| 18 | Token usage | Logged at `info` per call (`prompt_tokens`, `completion_tokens`, model, latency). Not stored. |
| 19 | Entry point | "Add item(s) with AI" on location and item pages when a key is configured; otherwise a muted "Set up AI" link to the settings tab. |
| 20 | Thumbnails keyed by bytes | The `thumbnails` table is re-keyed from `attachment_id` to `sha256`. Attachments and ingest photos sharing bytes share thumbnails; the failure cache is keyed by sha. |
| 21 | Settings screen | `/settings` with a tab bar; only the AI tab exists now. Types and Tags stay where they are. |

## 3. Architecture

New backend modules:

- `svc/ai_settings.rs`: read/write of the `ai.*` settings rows, env overrides, `AiConfig` (resolved values) and `AiSettingsView` (what GraphQL returns: never the key).
- `ai/`: `client.rs` (`AiClient` trait: `chat(ChatRequest) -> ChatResponse`), `openai.rs` (`OpenAiClient` on reqwest with rustls and webpki roots), `prompts.rs` (system prompts, JSON schemas), `fake.rs` (test client answering from a queue, `cfg(test)` plus the integration tests' support module).
- `svc/ingest.rs` (batches, items, photos: create/list/get/accept/skip/cleanup) and `ingest/runner.rs` (the pipeline: describe photos, synthesise, publish events).
- `ingest/events.rs`: an `IngestEvents` hub, one `tokio::sync::broadcast` channel per live batch, dropped when the batch finishes or nobody listens.
- `api/ingest.rs`: the staging upload and the SSE route.
- `svc/blob.rs`: `Blob { sha256, mime_type }` as the unit the thumbnail service works on.

Frontend additions: `page/SettingsPage.tsx` with `AiSettingsTab.tsx`, `page/IngestPage.tsx` with `IngestCollect.tsx`, `IngestReview.tsx`, `IngestDone.tsx`; hooks `useAiSettings`, `useAiSettingsMutations`, `useIngestBatch` (query plus SSE), `useIngestMutations`, and `useUploadPhoto` generalised to take the target URL.

## 4. Data model

### 4.1 Settings rows

| key | default | notes |
|---|---|---|
| `ai.base_url` | `https://api.openai.com/v1` | trailing slash stripped; must parse as an `http`/`https` URL |
| `ai.api_key` | absent | never returned; `""` or a missing row means "not configured" |
| `ai.vision_model` | `gpt-5-mini` | |
| `ai.synthesis_model` | `gpt-5-mini` | |
| `ai.extra_instructions` | absent | free text, at most 4000 characters |

Seeded by the migration except the key and the instructions. Environment:
`OPENAI_API_KEY` overrides `ai.api_key`, `OPENAI_BASE_URL` overrides
`ai.base_url`; when set, the corresponding update is refused with a message
naming the variable.

### 4.2 Thumbnails re-keyed

```sql
CREATE TABLE thumbnails_new (
  sha256 TEXT NOT NULL,
  size INTEGER NOT NULL,
  mime_type TEXT NOT NULL,
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  data BLOB NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (sha256, size)
);
INSERT OR IGNORE INTO thumbnails_new (sha256, size, mime_type, width, height, data, created_at)
  SELECT a.sha256, t.size, t.mime_type, t.width, t.height, t.data, t.created_at
  FROM thumbnails t JOIN attachments a ON a.id = t.attachment_id;
DROP TABLE thumbnails;
ALTER TABLE thumbnails_new RENAME TO thumbnails;
```

Thumbnail rows are removed when the last row referencing a sha (attachment
or ingest photo) goes away, in the same place the original file is removed.

### 4.3 Ingest tables

```sql
CREATE TABLE ingest_batches (
  id TEXT PRIMARY KEY NOT NULL,                 -- uuid v7
  parent_id TEXT REFERENCES entities(id) ON DELETE SET NULL,
  status TEXT NOT NULL,                          -- collecting | processing | reviewing | done
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE ingest_items (
  id TEXT PRIMARY KEY NOT NULL,
  batch_id TEXT NOT NULL REFERENCES ingest_batches(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  status TEXT NOT NULL,                          -- collecting | queued | analysing | ready | failed | accepted | skipped
  error TEXT,
  suggestion TEXT,                               -- JSON, present when ready
  entity_id TEXT REFERENCES entities(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE ingest_photos (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES ingest_items(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  size_bytes BIGINT NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL,                          -- pending | described | failed
  error TEXT,
  description TEXT,                              -- JSON from the vision step
  suggested_kind TEXT,                           -- photo | receipt | warranty | manual | other
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX ingest_items_batch ON ingest_items(batch_id, position);
CREATE INDEX ingest_photos_item ON ingest_photos(item_id, position);
CREATE INDEX ingest_photos_sha ON ingest_photos(sha256);
```

Status enums live in `kinds.rs` (`text_enum!`): `IngestBatchStatus`,
`IngestItemStatus`, `IngestPhotoStatus`, `SuggestedKind`.

**Sharer counting.** `svc::attachment::remove_original` counts sharers across
`attachments` and `ingest_photos`; the placing claim covers ingest uploads
too. Deleting the last sharer removes the file and the sha's thumbnail rows.

### 4.4 Suggestion JSON

Stored verbatim in `ingest_items.suggestion` and returned as
`IngestSuggestion`:

```json
{
  "name": "Logitech MX Master 3S",
  "description": "Wireless mouse, graphite",
  "manufacturer": "Logitech",
  "model_number": "MR0077",
  "serial_number": "2214LZ0A1B2C",
  "quantity": 1,
  "purchase_date": "2024-03-12",
  "purchase_from": "Amazon",
  "purchase_price_cents": 9999,
  "warranty_expires": "2026-03-12",
  "lifetime_warranty": false,
  "warranty_details": "2-year limited",
  "notes": "Receipt shows order #113-...",
  "tag_names": ["Electronics"],
  "confidence": "high",
  "reasoning": "Model text visible on the bottom label; price from the receipt."
}
```

Every field is optional and `null` means "not found". Dates are ISO
`YYYY-MM-DD`; money is integer cents in the instance currency; `quantity` is
a non-negative number; `tag_names` is filtered to names that exist
(case-insensitive) before storing. `confidence` is one of `low`, `medium`,
`high`.

### 4.5 Photo description JSON

Stored in `ingest_photos.description`:

```json
{
  "kind": "receipt",
  "summary": "Amazon receipt dated 2024-03-12 for a Logitech MX Master 3S",
  "text": "…transcription of visible text…",
  "details": {
    "item_name": null, "brand": "Logitech", "model_number": null,
    "serial_number": null, "price": "99.99", "currency": "USD",
    "date": "2024-03-12", "vendor": "Amazon", "warranty": null, "other": null
  }
}
```

## 5. Pipeline

`submitIngestBatch` moves the batch to `processing`, each item with at least
one photo to `queued` (items with no photos are deleted), and hands the batch
id to the runner. The runner:

1. For each item (parallel), for each `pending` or `failed` photo (parallel,
   under the global semaphore): get or generate the 1200 px thumbnail, build
   the vision request (system prompt from `prompts.rs`, extra instructions,
   the image as a data URL), parse the description JSON, store it with
   `status = described` and `suggested_kind`, else `failed` with a short
   error (no paths, no key). Publish `photo` event.
2. When all of the item's photos have settled: if none is described, the
   item is `failed`; else build the synthesis request (all descriptions,
   the field list with types, the currency, the existing tag names), parse
   the suggestion, filter tags, store it, item `ready`. Publish `item` event.
3. When every item has settled: batch `reviewing`. Publish `batch` event.

`retryIngestItem` re-queues one `failed` (or `ready`) item and runs steps 1
and 2 for it. Accepting or skipping the last open item moves the batch to
`done`. `deleteIngestBatch` removes everything (files refcounted).

The runner holds no DB connection across a model call. Model calls run on the
async runtime (reqwest); DB work runs in `spawn_blocking`. A server restart
mid-batch leaves items `analysing`; on startup the runner re-queues any
`analysing` items of `processing` batches.

## 6. GraphQL

```graphql
type AiSettings {
  baseUrl: String!
  visionModel: String!
  synthesisModel: String!
  extraInstructions: String
  hasApiKey: Boolean!
  fromEnvironment: [String!]!   # e.g. ["OPENAI_API_KEY"]
}
input AiSettingsInput {
  baseUrl: String!
  visionModel: String!
  synthesisModel: String!
  extraInstructions: String
  apiKey: String   # omitted/null: keep; "": clear; otherwise: replace
}
type AiTestResult { ok: Boolean!, message: String!, latencyMs: Int! }

enum IngestBatchStatus { COLLECTING PROCESSING REVIEWING DONE }
enum IngestItemStatus { COLLECTING QUEUED ANALYSING READY FAILED ACCEPTED SKIPPED }
enum IngestPhotoStatus { PENDING DESCRIBED FAILED }
enum SuggestedKind { PHOTO RECEIPT WARRANTY MANUAL OTHER }

type IngestBatch {
  id: ID!, parentId: ID, status: IngestBatchStatus!
  items: [IngestItem!]!, createdAt: DateTime!, updatedAt: DateTime!
}
type IngestItem {
  id: ID!, position: Int!, status: IngestItemStatus!, error: String
  suggestion: IngestSuggestion, entityId: ID, photos: [IngestPhoto!]!
}
type IngestSuggestion {
  name: String, description: String, manufacturer: String, modelNumber: String
  serialNumber: String, quantity: Float, purchaseDate: LocalDate
  purchaseFrom: String, purchasePriceCents: Int, warrantyExpires: LocalDate
  lifetimeWarranty: Boolean, warrantyDetails: String, notes: String
  tagNames: [String!]!, confidence: String, reasoning: String
}
type IngestPhoto {
  id: ID!, position: Int!, status: IngestPhotoStatus!, error: String
  title: String!, mimeType: String!, sizeBytes: Int!
  url: String!, thumbnailUrl(size: Int = 500): String
  suggestedKind: SuggestedKind, summary: String, text: String
}
input IngestPhotoKindInput { photoId: ID!, kind: AttachmentKind! }

extend type Query {
  aiSettings: AiSettings!
  ingestBatch(id: ID!): IngestBatch
  openIngestBatches(parentId: ID): [IngestBatch!]!   # status != DONE, newest first
}
extend type Mutation {
  updateAiSettings(input: AiSettingsInput!): AiSettings!
  testAiConnection: AiTestResult!
  createIngestBatch(parentId: ID): IngestBatch!      # with one empty item
  addIngestItem(batchId: ID!): IngestItem!
  removeIngestItem(id: ID!): Boolean!
  removeIngestPhoto(id: ID!): Boolean!
  submitIngestBatch(id: ID!): IngestBatch!
  retryIngestItem(id: ID!): IngestItem!
  acceptIngestItem(id: ID!, input: EntityInput!, photoKinds: [IngestPhotoKindInput!]!): Entity!
  skipIngestItem(id: ID!): IngestItem!
  deleteIngestBatch(id: ID!): Boolean!
}
```

Rules: every mutation starts with `ctx.require_write()`. `acceptIngestItem`
creates the entity through the existing `svc::entity::create`, then inserts
one attachment per photo (kind from `photoKinds`, default the suggested kind
mapped `other -> attachment`; missing entries default too), primary = first
attachment of kind `photo` (else none), then deletes the ingest photo rows
(files stay: the attachments share the sha). `EntityInput.parent_id` is the
user's choice; the batch's parent is only the default. `acceptIngestItem` and
`skipIngestItem` refuse items not in `ready` or `failed`. `submitIngestBatch`
refuses a batch with no photos at all. Mutations that touch a batch bump
`updated_at`.

## 7. HTTP endpoints

| Route | Method | Behaviour |
|---|---|---|
| `/api/ingest/items/{itemId}/photos` | POST multipart `file` | Same pipeline as `/api/upload` (sniff, temp file, claim, dedupe, 300 px thumbnail generated, same status codes and limits) but stores an `ingest_photos` row. 404 for an unknown item, 409 if the item's batch is not `collecting`. Returns 201 with the `IngestPhoto` JSON shape (`id, position, status, title, mimeType, sizeBytes, url, thumbnailUrl`). |
| `/ingest/photos/{id}` | GET | The original, served exactly as `/attachments/{id}` (ETag, immutable, nosniff, CSP sandbox, `?v=`). |
| `/ingest/photos/{id}/thumb/{size}` | GET | As `/attachments/{id}/thumb/{size}`. |
| `/api/ingest/batches/{id}/events` | GET | `text/event-stream`. Events `photo`, `item`, `batch` with data `{"id": "..."}`, plus a `ping` comment every 15 s. Exempt from compression. Ends when the batch is `done` or deleted. 404 for an unknown batch. |

The attachment and ingest photo handlers share one implementation over a
`BlobSource` (id -> `Blob` plus title/mime) so behaviour cannot drift.

## 8. Prompts

Held in `ai/prompts.rs` as constants with unit tests that pin their key
sentences. The vision system prompt: describe one photograph of a household
item or a document about it; classify the kind; transcribe visible text
exactly; extract brand, model, serial, price, currency, date, vendor,
warranty terms when present; answer only with the JSON object of §4.5. The
synthesis system prompt: given descriptions of one item's photos, the field
list, the instance currency and the existing tags, fill the item record of
§4.4 with `null` for anything not supported by the photos; never invent
serials or prices; prices in integer cents of the instance currency; tags
only from the given list. Extra instructions are appended as a final
paragraph of each system prompt.

## 9. Frontend

- **Settings**: `/settings` redirects to `/settings/ai`. `SettingsPage`
  renders a tab bar (`role="tablist"`) and the active tab. `AiSettingsTab`:
  base URL, vision model, synthesis model, extra instructions (textarea),
  API key (`type="password"`, blank; a "Key saved" note and a "Clear key"
  button when `hasApiKey`), env-provided fields disabled with "set from
  environment"; Save; Test connection with the result inline. Nav gains
  "Settings".
- **Entry**: `LocationPage` and `ItemPage` show "Add item(s) with AI" when
  `hasApiKey`, else a muted "Set up AI" link. Clicking creates a batch with
  the page's entity as parent and navigates to `/ingest/:batchId`. If
  `openIngestBatches(parentId)` is non-empty the page also shows "Resume
  batch" links.
- **`/ingest/:batchId`** (`IngestPage`) switches on the batch status:
  - `collecting` (`IngestCollect`): item groups in order, each with its
    photos (thumb 300, remove), "Add photos" (file input, `accept="image/*"`,
    multiple) and "Take photo" (`capture="environment"`), the uploader's
    per-file status list; "Next item" adds an empty group; "Remove item";
    "Submit" enabled when any photo exists. Uploads go through
    `useUploadPhoto` pointed at the item's staging URL.
  - `processing`/`reviewing` (`IngestReview`): a progress strip (one chip
    per item with its status) and the review pane for the first item in
    `ready` or `failed` that is not yet accepted/skipped: photos strip with
    a kind select per photo and a collapsible "What the AI saw" (summary and
    transcribed text), `EntityForm` prefilled from the suggestion with the
    confidence and reasoning shown above it, Save (accept) and Skip; a
    failed item shows its error with Retry and Skip. While nothing is ready
    yet, a waiting state.
  - `done` (`IngestDone`): counts of saved and skipped items with links to
    the saved entities; "Add more items" (new batch, same parent) and "Back
    to <parent>".
- Live updates: `useIngestBatch(id)` runs the query and opens an
  `EventSource` on the events URL; every event and every `open` refetches
  the query (debounced 150 ms). After accept, the usual refetch policy runs
  for the created entity's parent.
- Copy: "Add item(s) with AI", "Next item", "Take photo", "Add photos",
  "Submit for analysis", "Save item", "Skip", "Retry", "Add more items".

## 10. Authentication readiness

AI settings mutations and the test call require write today and will become
admin-only when roles arrive; the ingest routes read `Extension<Actor>` like
the upload route. Ingest batches carry no owner in v1; an owner column can
be added when users exist.

## 11. Configuration and operations

- `OPENAI_API_KEY`, `OPENAI_BASE_URL` (optional) documented in README and
  `env.prod` as comments.
- reqwest with `rustls-tls-webpki-roots` so the scratch image needs no CA
  store. `cargo tree -i openssl` stays empty.
- Outbound calls log the model, latency, token usage and the HTTP status at
  `info`; error bodies at `warn` truncated to 500 characters; never the key.
- The cleanup job logs how many batches it removed.

## 12. Testing

- `FakeAiClient` answers from a queue of responses and records requests; the
  runner and the GraphQL flow are tested end to end with it.
- `OpenAiClient` is tested against a fake OpenAI server (an axum app on a
  random port inside the test): request shape (auth header, model, image
  part, `response_format`), the `json_schema -> json_object` fallback, 429
  retry, timeout, and that the key never appears in an error message.
- Settings: key write-only (query never returns it, the DB holds it), clear
  with `""`, env override refusal with the variable named.
- Ingest: staging upload shares the placing claim and refcount with
  attachments (a sha shared by an attachment and an ingest photo survives
  either delete); accept moves photos to attachments with the right kinds
  and primary; skip removes files; cleanup removes only stale batches;
  restart re-queues `analysing` items; the SSE route emits the three event
  types and ends on `done`.
- Frontend: settings tab (key never echoed, env-disabled fields, test
  result), collect flow (groups, next item, remove, submit gating), review
  flow (prefill, kind select, save calls accept with the edited input and
  kinds, skip, retry), done flow, `useIngestBatch` refetch on events and
  reconnect (EventSource mocked).

## 13. Invariants this feature depends on

- `is_thumbnailable` remains the single MIME gate; the 1200 px rendition
  exists for every stored photo because upload generates a thumbnail (base
  spec, phase 5).
- Thumbnail rows and original files are keyed by sha256 and removed only
  when no `attachments` or `ingest_photos` row shares the sha.
- `EntityInput` is the single write shape for entities; the review form
  emits it unchanged.
- `/graphql` is POST-only; the SSE route is GET and read-only.
- Attachment URLs carry `?v=<sha prefix>`; ingest photo URLs do the same.

## 14. Phases

**Phase 6: settings and AI client.** Thumbnails re-keyed by sha256 with the
`BlobSource` refactor; `ai.*` settings with env overrides; `AiClient`,
`OpenAiClient`, `FakeAiClient`, prompts; `aiSettings`, `updateAiSettings`,
`testAiConnection`; settings screen with the AI tab and nav link. Exit: a key
saved in the UI is never shown again, "Test connection" succeeds against a
fake OpenAI server in tests and against the real endpoint in acceptance, and
every existing attachment thumbnail still serves after the migration.

**Phase 7: ingest workflow.** Ingest tables and services, staging upload and
photo routes, the runner with the semaphore and retries, SSE events, cleanup
and restart re-queue, the GraphQL surface, the collect/review/done pages and
entry points. Exit: from a location page, two items with two photos each are
submitted, analysed with the fake client in tests and the real model in
acceptance, reviewed, one saved with its photos and kinds and one skipped,
and the location shows the new item with its primary photo.
