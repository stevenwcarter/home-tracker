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
| 2 | API style | Chat Completions (`POST {base_url}/chat/completions`) with `image_url` content parts. Synthesis asks for `response_format: {type: "json_schema", strict}`; if the endpoint rejects `json_schema` (HTTP 400 whose body mentions `response_format`), retry once with `{type: "json_object"}` and validate the JSON ourselves. The output cap goes out as `max_completion_tokens` (current OpenAI models refuse `max_tokens`); on an HTTP 400 whose body names `max_completion_tokens`, resend once with `max_tokens` for older compatible servers. Neither resend uses up a retry. |
| 3 | Key storage | Plaintext in the `settings` table. The SQLite file is treated as secret (documented). Encryption at rest can follow later without a schema change. |
| 4 | Environment overrides | `OPENAI_API_KEY` and `OPENAI_BASE_URL` win over the stored values when set. The AI tab shows those fields as "set from environment" and disabled. |
| 5 | Models | Two settings: `vision_model` (per-photo step) and `synthesis_model` (combine step), both default `gpt-5-mini`. Free-text fields. |
| 6 | Extra instructions | One optional free-text setting appended to both system prompts. |
| 7 | Test connection | A mutation that sends a tiny text-only chat completion to the synthesis model ("Reply with the single word OK.", capped at 8 tokens) and reports ok/failure with latency: `Connected: <model> answered in <n> ms`, or the `AiError` message. Without a key it answers `ok: false` without calling out. |
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
- `ai/`: `client.rs` (`AiClient` trait: `chat(ChatRequest) -> ChatResponse`), `openai.rs` (`OpenAiClient` on reqwest with rustls and webpki roots), `prompts.rs` (system prompts, JSON schemas), `fake.rs` (`FakeAiClient`, a test client answering from a queue; public rather than `cfg(test)` so the integration tests can use it), `env.rs` (`AiEnv`, the two `OPENAI_*` overrides). `ai/mod.rs` holds `AiState { env, client }`, which the router hands to every GraphQL context (`AiState::disabled()` for routers built without a real client).
- `svc/ingest.rs` (batches, items, photos: create/list/get/accept/skip/cleanup) and `ingest/runner.rs` (the pipeline: describe photos, synthesise, publish events).
- `ingest/events.rs`: an `IngestEvents` hub, one `tokio::sync::broadcast` channel per live batch, dropped when the batch finishes or nobody listens.
- `api/ingest.rs`: the staging upload and the SSE route.
- `svc/blob.rs`: `Blob { sha256, mime_type }` as the unit the thumbnail service works on.

Frontend additions: `page/SettingsPage.tsx` with `AiSettingsTab.tsx`, `page/IngestPage.tsx` with `IngestCollect.tsx`, `IngestReview.tsx`, `IngestDone.tsx`; hooks `useAiSettings`, `useAiSettingsMutations`, `useIngestBatch` (query plus SSE), `useIngestMutations`, and `useUploadPhoto` generalised to take the target URL.

## 4. Data model

### 4.1 Settings rows

| key | default | notes |
|---|---|---|
| `ai.base_url` | `https://api.openai.com/v1` | trimmed, trailing slashes stripped; must be an `http`/`https` URL with a host, no `user:password@` part, no query string and no fragment; the error never echoes the input |
| `ai.api_key` | absent | never returned; `""` or a missing row means "not configured" |
| `ai.vision_model` | `gpt-5-mini` | |
| `ai.synthesis_model` | `gpt-5-mini` | |
| `ai.extra_instructions` | absent | free text, at most 4000 characters |

Seeded by the migration except the key and the instructions; a missing
seeded row is an error, never silently replaced by the default. Environment:
`OPENAI_API_KEY` overrides `ai.api_key`, `OPENAI_BASE_URL` overrides
`ai.base_url` (blank values count as unset). When a variable is set, an
update that would change its field is refused with a message naming the
variable, and the other fields still save: with `OPENAI_API_KEY` set, any
`apiKey` other than omitted/null is refused, including `""`; with
`OPENAI_BASE_URL` set, `baseUrl` (a required input field) is accepted only
when it normalises to the environment value, which is what the screen shows
and sends back, and the stored row is then left alone. An invalid
`OPENAI_BASE_URL` fails reads and model calls with a message naming the
variable rather than falling back to the stored URL.

The key never follows the endpoint to another origin. When an update's
normalised base URL differs from the stored one in scheme, host or port
(host compared case-insensitively, a missing port read as the scheme's
default), a stored key is cleared unless the update carries a new `apiKey`,
and the response's `hasApiKey` turns false; the screen then says "Changing
the endpoint host cleared the saved key. Enter it again to keep using AI."
While the key comes from `OPENAI_API_KEY` (and the base URL does not come
from `OPENAI_BASE_URL`), such a change is refused with a message naming
`OPENAI_API_KEY`, since the screen cannot clear an environment key. A
change of path alone keeps the key.

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
id to the runner (`IngestRunner::spawn_batch`, after the submitting
transaction has committed and its connection is released). Submit itself
publishes no event: the submitting client has the mutation's answer, and the
runner's first `item` events follow at once. The runner (`src/ingest/runner.rs`):

1. For each item (a parallel task): claim it with
   `svc::ingest::claim_for_analysis`, a conditional
   `UPDATE … SET status = 'analysing' WHERE status = 'queued'` that also
   clears any previous suggestion and error. An item in any other status
   (already claimed by a racing run, accepted, skipped) is left alone, so two
   runs never analyse one item twice. Publish `item` event.
2. For each of its photos that is not `described` (parallel tasks, each
   model call under the process-wide semaphore of 4, held only around the
   call): get or generate the 1200 px WebP thumbnail, build the vision
   request (system prompt from `prompts.rs`, extra instructions, the image
   as a `data:image/webp;base64,…` URL with `detail: auto`), parse the
   description JSON leniently (`ingest/parse.rs`), store it with
   `status = described` and `suggested_kind`, else `failed` with a short
   readable error (no paths, no key). A panicking or failing task fails only
   its own photo. Publish `photo` event.
3. When all of the item's photos have settled: if none is described, the
   item is `failed`; else build the synthesis request (all descriptions,
   the field list with types, the currency, the existing tag names), parse
   the suggestion, filter tags to existing ones case-insensitively, store
   it, item `ready`. No key configured fails the item with "AI is not
   configured: add an API key first". Publish `item` event.
4. After each item settles, `settle_batch` (inside an immediate
   transaction, so concurrent items publish at most one change) moves the
   batch to `reviewing` once no item is `queued`/`analysing`, and to `done`
   once every item is `accepted`/`skipped`. Publish `batch` event only when
   the status actually changed; close the batch's channel when it is `done`.

The runner adds no retries of its own beyond the client's (two backoff
retries on 429/5xx per call). A batch of N photos in M items costs N vision
calls plus M synthesis calls.

`retryIngestItem` works on a `ready` or `failed` item only (else
`NotRetryable`, "only an item that is ready or failed can be retried"): it
re-queues the item, clears its error, and moves a `reviewing` batch back to
`processing` (so a restart mid-retry still finds it through the re-queue
below); then the runner re-describes only the photos that are not
`described` and synthesises again from all described ones, and settles the
batch back to `reviewing`. Accept, skip and retry each publish an `item`
event, then a `batch` event when the batch status changed, and close the
channel when the batch is now `done`. Accepting or skipping the last open
item moves the batch to `done`. `deleteIngestBatch` removes everything (files
refcounted) and closes the channel.

The runner holds no DB connection across a model call. Model calls run on the
async runtime (reqwest); DB work runs in `spawn_blocking`. A server restart
mid-batch leaves items `analysing`; on startup (after the stale-batch
cleanup, so an abandoned batch is not resumed and then deleted) the server
resets the `analysing` items of every `processing` batch to `queued` and
runs each such batch again in the background. Cleanup deletes batches with
no activity for 7 days (`updated_at`, bumped by every mutation and upload
that touches the batch, including accept and skip) at startup and every
24 h, logs the count, and closes the deleted batches' channels.

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

Rules: there are nine ingest mutations (`createIngestBatch` through
`deleteIngestBatch` above), and every mutation starts with
`ctx.require_write()`. `acceptIngestItem`
creates the entity through the existing `svc::entity::create`, then inserts
one attachment per photo (kind from `photoKinds`, default the suggested kind
mapped `other -> attachment`, or `photo` when nothing was suggested; missing
entries default too), primary = first attachment of kind `photo` whose MIME
passes `is_thumbnailable` (else none), then deletes the ingest photo rows
(files stay: the attachments share the sha), all in one immediate
transaction. A `photoKinds` entry naming a photo of another item is refused
(`ForeignPhoto`). `EntityInput.parent_id` is the
user's choice; the batch's parent is only the default. `acceptIngestItem` and
`skipIngestItem` refuse items not in `ready` or `failed`; `retryIngestItem`
likewise (see §5). `addIngestItem`, `removeIngestItem` and
`removeIngestPhoto` work only while the batch is `collecting`.
`submitIngestBatch` refuses a batch with no photos at all. A refusal
(`IngestError`: not found, not collecting, not reviewable, not retryable,
empty, foreign photo) comes back as a field error with its own message and is
logged at `info`, not as a server error. Mutations that touch a batch bump
`updated_at`. `testAiConnection` loads the resolved config and releases its
database connection before awaiting the model.

The handler builds the context with `GraphQLContext::for_request(pool,
actor, data_dir, ai, ingest)` from the router's shared `AiState` and
`Arc<IngestRunner>`, so resolvers spawn work on the same runner (and publish
on the same events hub) that `serve` resumes and the SSE route reads.
`GraphQLContext::new` (tests) builds a disabled runner of its own.

## 7. HTTP endpoints

| Route | Method | Behaviour |
|---|---|---|
| `/api/ingest/items/{itemId}/photos` | POST multipart `file` | Same pipeline as `/api/upload` (sniff, temp file, claim, dedupe, 300 px thumbnail generated, same status codes and limits) but stores an `ingest_photos` row. 404 for an unknown item, 409 if the item's batch is not `collecting`. Returns 201 with the `IngestPhoto` JSON shape (`id, position, status, title, mimeType, sizeBytes, url, thumbnailUrl`). |
| `/ingest/photos/{id}` | GET | The original, served exactly as `/attachments/{id}` (ETag, immutable, nosniff, CSP sandbox, `?v=`). |
| `/ingest/photos/{id}/thumb/{size}` | GET | As `/attachments/{id}/thumb/{size}`. |
| `/api/ingest/batches/{id}/events` | GET | `text/event-stream`. Events `photo`, `item`, `batch` with data `{"id": "..."}`, plus a `ping` comment every 15 s. Exempt from compression. Ends when the batch is `done` or deleted. 404 for an unknown batch. |

SSE rules: the handler subscribes first, then loads the batch (in
`spawn_blocking`), so no event between the load and the subscription is
lost. An unknown batch answers 404 (500 when the load fails) and closes the
channel it just opened, so the hub keeps no entry for it; the client's
`useIngestBatch` reports that as `notFound`. A batch that is already `done`
gets exactly one `batch` event and the stream ends (the client does not
reopen it). A receiver that lags behind the 64-event buffer gets one `batch`
event for the batch instead of the missed ones, since every event only tells
the client to refetch. The stream ends when the channel closes: on `done`
(runner or review step), on `deleteIngestBatch`, and when cleanup deletes the
batch. `publish` never opens a channel, and drops one whose last receiver is
gone, so an unwatched batch costs nothing.

The attachment and ingest photo handlers share one implementation
(`api/blob.rs`: `serve_original`, `serve_thumb` over a `Blob`) so behaviour
cannot drift; a test compares the two routes' header sets for the same
bytes.

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

The user messages are built by `describe_user_message` ("Photo N of M of one
item.", beside the image part) and `synthesis_user_message` (each described
photo as "Photo N: <description JSON>", the currency, the tag names and the
`SUGGESTION_FIELDS` list with types, pinned against
`item_suggestion_schema()` by a test). Both calls ask for the §4.4/§4.5
schemas and cap output at `DESCRIBE_MAX_TOKENS` / `SYNTHESIS_MAX_TOKENS`
(6000 each, reasoning tokens included). The parser takes the whole answer as
JSON first, else the first balanced `{…}` object in it (code fences, prose
around it); a non-object, truncated or summary-less answer fails the photo
or item with a readable `ParseError` message. Numeric strings are coerced;
negative, fractional-cent or out-of-range numbers and bad dates are dropped
rather than failing the item.

## 9. Frontend

- **Settings**: `/settings` redirects to `/settings/ai`. `SettingsPage`
  renders a tab bar (`role="tablist"`) and the active tab. `AiSettingsTab`:
  base URL, vision model, synthesis model, extra instructions (textarea),
  API key (`type="password"`, always blank; a "Key saved" note and a
  "Clear key" button when `hasApiKey` and the key is not from the
  environment), env-provided fields disabled with "Set from OPENAI_... in
  the environment"; Save (it omits `apiKey` unless a key was typed or
  cleared, and sends blank extra instructions as null); Test connection
  (disabled without a key) with the result inline in an `aria-live` region.
  Nav gains "Settings".
- **Entry** (`components/ingest/AiEntry.tsx`): `LocationPage` and
  `ItemPage` show "Add item(s) with AI" when `hasApiKey`, else a muted "Set
  up AI" link to `/settings/ai`. Clicking creates a batch with the page's
  entity as parent and navigates to `/ingest/:batchId`, through
  `useStartBatch` (an in-flight guard, so a double tap creates one batch).
  If `openIngestBatches(parentId)` is non-empty the page also shows "Resume
  batch (N items, started X ago)" links.
- **`/ingest/:batchId`** (`IngestPage`, heading "Add items with AI",
  breadcrumb through the parent once it has loaded) switches on the batch
  status, and shows NotFound for an unknown or deleted batch. Items are
  labelled by their index in the list ("Item 2"), never by `position`,
  which keeps gaps after removals.
  - `collecting` (`IngestCollect`, `IngestItemGroup`): item groups in
    order, each with its photos (thumb 300 in a wrapping 3/4/6-column grid,
    per-photo remove), "Add photos" (file input, `accept="image/*"`,
    multiple) and "Take photo" (a second input with `capture="environment"`),
    the uploader's per-file status list; "Next item" adds an empty group;
    "Remove item" (immediate for an empty item, confirmed in a
    `ConfirmDialog` for one with photos; hidden when only one item is left);
    "Submit for analysis" enabled when any photo exists and no upload is in
    flight (`PhotoUploader`'s `onUploadingChange`; "Waiting for uploads to
    finish" meanwhile); "Discard batch" (confirmed, deletes the batch and
    goes back). Uploads go through `useUploadPhoto` with an `ingestItem`
    target (the item's staging URL, no `primary`, refetching only
    `GetIngestBatch`). A reload lands back on the same batch with
    everything intact, since it all lives on the server.
  - `processing`/`reviewing` (`IngestProgress` and `IngestReview`): a
    progress strip (`aria-live`, one chip per item, "Item N: Queued /
    Analysing / Ready / Failed / Saved / Skipped", `aria-current` on the
    item under review) and the review pane for `nextReviewable(batch)`,
    the first item in `ready` or `failed`: photos strip with a "Kind of
    photo N" select per photo (default: the suggested kind) and a
    collapsible "What the AI saw" (summary and transcribed text),
    "Confidence: x. reasoning" above an `EntityForm` in create mode seeded
    via `initialInput` from `suggestionToInput` (Item type, the batch's
    parent, tags resolved by name; the input is fixed once the type and tag
    lookups first load, so a later `GetTags` refetch does not remount the
    form and lose edits), "Save item" (accept) and "Skip". A failed item
    shows its error with Retry and Skip and no form. While nothing is ready
    yet, a waiting state "Analysing your photos…".
  - `done` (`IngestDone`): "Saved N items, skipped M." with links to the
    saved entities by their saved names; "Add more items" (new batch, same
    parent, through `useStartBatch`) and "Back to <parent>" (Home when the
    batch has no parent or it is gone).
- Live updates: `useIngestBatch(id)` runs the query and opens an
  `EventSource` on the events URL; every event and every `open` refetches
  the query (one trailing refetch, debounced 150 ms). It closes the stream
  (and does not open one) once the batch is `DONE`, because the server ends
  a done batch's stream and a browser would otherwise reconnect forever; a
  404 stops the browser by itself. `useIngestMutations` holds the nine
  writes, all refetching `GetIngestBatch`/`GetOpenIngestBatches`; after
  accept, the usual entity refetch policy runs for the chosen parent.
- Copy: "Add item(s) with AI", "Next item", "Take photo", "Add photos",
  "Submit for analysis", "Save item", "Skip", "Retry", "Add more items".

## 10. Authentication readiness

AI settings mutations and the test call require write today and will become
admin-only when roles arrive; the ingest routes read `Extension<Actor>` like
the upload route. Ingest batches carry no owner in v1; an owner column can
be added when users exist.

Until then every LAN user can write, which the settings rulings take into
account: an endpoint host change clears a saved key (§4.1), so nobody can
redirect the stored key to a host of their choosing; but Test connection
calls whatever host the writer saved and shows up to 500 characters of its
reply. That second point is documented, not blocked: anyone who can write
can already point the app anywhere, and it becomes admin-only with roles.

## 11. Configuration and operations

- `OPENAI_API_KEY`, `OPENAI_BASE_URL` (optional) documented in README and
  `env.prod` as comments.
- reqwest with `rustls-tls-webpki-roots` so the scratch image needs no CA
  store. `cargo tree -i openssl` stays empty.
- Outbound calls log the model, latency, token usage and the HTTP status at
  `info`; error bodies at `warn`, with the key replaced by `***` and then
  truncated to 500 characters; never the key. Transport errors are reported
  without the request URL. The full key never appears; OpenAI's own error
  bodies may echo a masked fragment, which we also mask: any
  `sk-[A-Za-z0-9_-]*\*{3,}[A-Za-z0-9_-]*` token, and the key's first 8
  characters followed by `*`s.
- Timeouts: 60 s per request (connect to last body byte), 10 s to connect;
  a timeout is not retried.
- The cleanup job logs how many batches it removed.
- Phase 7 note: reasoning models count their reasoning tokens against
  `max_completion_tokens`, so the ingest caps must leave room for reasoning
  on top of the JSON answer. The runner's own retries
  compound with the client's (two backoff retries per call), so a runner
  retry budget multiplies the worst-case calls and wait per item.

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
`Blob` refactor; `ai.*` settings with env overrides; `AiClient`,
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
