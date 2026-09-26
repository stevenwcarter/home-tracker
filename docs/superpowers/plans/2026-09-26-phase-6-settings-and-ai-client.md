# Phase 6: Settings screen and AI client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-key thumbnails by content hash, store OpenAI-compatible AI settings with a write-only key and environment overrides, add an `AiClient` with an OpenAI implementation and a fake, and ship a settings screen with an AI tab and a "Test connection" button.

**Architecture:** Thumbnails become a property of bytes (`sha256`) so attachments and, in Phase 7, staged ingest photos share them; the thumbnail service works on a `Blob { sha256, mime_type }`. AI settings live in the existing `settings` table under `ai.*` keys; `AiEnv` carries the two environment overrides and is threaded through the router into the GraphQL context together with an `Arc<dyn AiClient>`. The client is stateless apart from its reqwest client: every call receives the resolved `AiConfig`, so settings changes apply immediately.

**Tech Stack:** Rust 2024, axum 0.8, juniper 0.17 (async resolvers), diesel 2.3 + SQLite, reqwest 0.12 (`rustls-tls-webpki-roots`), async-trait, React 19 + Apollo 4 + vitest.

**Spec:** `docs/superpowers/specs/2026-09-26-ai-ingest-design.md` (§2 decisions 3–7, 20, 21; §3; §4.1–4.2; §6 `AiSettings`, `updateAiSettings`, `testAiConnection`; §9 Settings; §11; §12; §14 Phase 6). Base spec: `docs/superpowers/specs/2026-09-25-home-tracker-design.md`.

## Global Constraints

- Rust edition 2024; `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` clean; `cargo tree -i openssl` and `cargo tree -i native-tls` find nothing.
- `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls-webpki-roots", "json", "http2"] }`; `async-trait = "0.1"`. No `rustls` provider conflict (`cargo build` must not fail on "no process-level CryptoProvider"; if it does, pin `rustls` with the `ring` feature and call `CryptoProvider::install_default` once in `main`).
- Settings keys (spec §4.1): `ai.base_url` (default `https://api.openai.com/v1`), `ai.api_key`, `ai.vision_model` (default `gpt-5-mini`), `ai.synthesis_model` (default `gpt-5-mini`), `ai.extra_instructions` (max 4000 chars). Environment: `OPENAI_API_KEY`, `OPENAI_BASE_URL`.
- The API key never leaves the server: not in GraphQL responses, not in error messages, not in logs.
- Every mutation calls `ctx.require_write()` first.
- Model calls: 60 s request timeout, 10 s connect timeout, retries on 429/5xx with backoff 1 s then 4 s (injectable for tests), `json_schema -> json_object` fallback on a 400 whose body mentions `response_format`. Token usage logged at `info`; error bodies at `warn`, truncated to 500 chars.
- Frontend: semantic theme classes only; every GraphQL operation in a hook; no `window.confirm`; `yarn`, never npm. CLAUDE.md under 120 lines, no em dashes.
- `homebox/`, `homebox-backup/`, `site/build/`, `data/` are never `git add`ed. `site/build/index.html` must exist before cargo commands. Cargo target dir `/home/.build/cargo-target`.
- Every commit ends with `Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF`.

## Review Focus

1. An existing database with stored thumbnails: after the migration every thumbnail still serves at the same URL with the same bytes, and the importer's re-import of an attachment whose bytes changed leaves no orphaned thumbnail rows. Task 1 tests.
2. A saved API key: no query, mutation response, log line or error message ever contains it, including the client's error for a 401 whose body echoes the header. Tasks 2 and 3 tests.
3. `OPENAI_API_KEY` set in the environment: the query reports it, the update of that field is refused naming the variable, the other fields still save. Task 2 test.
4. A base URL with a trailing slash or a path (`https://host/v1/`): the request goes to `https://host/v1/chat/completions` exactly once. Task 3 test.
5. A provider that rejects `json_schema`: the call succeeds via `json_object` on the second attempt, and a provider that returns 429 twice then 200 succeeds without the caller seeing an error. Task 3 tests.

---

### Task 1: Thumbnails keyed by sha256

**Files:**
- Create: `migrations/2026-09-26-100000_thumbnails_by_sha/{up,down}.sql`, `src/svc/blob.rs`
- Modify: `src/schema.rs` (regenerate), `src/models/thumbnail.rs`, `src/svc/attachment.rs`, `src/svc/thumbnail.rs` (`Generated::into_row(sha256, ...)`), `src/svc/thumbnail_service.rs`, `src/svc/upload.rs`, `src/api/attachments.rs`, `src/import/run.rs`, `src/svc/fixtures.rs`, `src/svc/mod.rs`, `tests/attachments.rs`, `tests/import.rs`, `tests/upload.rs`, `CLAUDE.md` (thumbnail bullets)

**Interfaces:**
- Produces: `svc::blob::Blob { pub sha256: String, pub mime_type: String }` with `impl From<&Attachment> for Blob`; `svc::attachment::{thumbnail(conn, sha256, size), insert_thumbnail(conn, &Thumbnail), delete_thumbnails(conn, sha256)}`; `ThumbnailService::get_or_generate(&self, blob: &Blob, size: ThumbSize) -> Result<Option<Thumbnail>>`; `Thumbnail { sha256, size, mime_type, width, height, data, created_at }`; `remove_original` also deletes the sha's thumbnail rows when the last sharer goes.
- Consumed by Phase 7 (ingest photos are `Blob`s).

- [ ] **Step 1: Migration.** `up.sql` exactly as spec §4.2 (create `thumbnails_new`, copy via join, drop, rename). `down.sql` recreates the old shape and copies back via the join on `attachments.sha256` (one row per attachment sharing the sha). Run `diesel migration run && diesel print-schema > src/schema.rs` (schema loses the `joinable!` line for thumbnails).
- [ ] **Step 2: Failing tests.**
  - `tests/attachments.rs::existing_thumbnails_survive_the_migration`: build a pool with `db::build_pool` on a temp file, run every pending migration except `2026-09-26-100000_thumbnails_by_sha` (use `MigrationHarness::pending_migrations` and `run_migration` in order, skipping by `name()`), insert an entity, an attachment with sha `S`, and a thumbnail row `(attachment_id, 500, image/webp, 10, 8, b"webp-bytes")` through raw SQL, run the last migration, then assert `thumbnails` has exactly one row `(S, 500)` with the same bytes and dimensions, and that `GET /attachments/{id}/thumb/500` serves those bytes with `generations() == 0`.
  - `tests/attachments.rs::two_attachments_sharing_bytes_share_one_thumbnail`: two attachments with the same sha; request `/thumb/300` on both; `generations() == 1`; one `thumbnails` row.
  - `svc::attachment` unit test `deleting_the_last_sharer_removes_its_thumbnails`: two attachments share sha; delete one → thumbnail rows remain; delete the other → rows gone and file gone.
  - `tests/import.rs::a_reimport_with_changed_bytes_leaves_no_orphaned_thumbnails`: import the mini backup (photo with thumbnail), rewrite the photo bytes in a second backup dir, import again → the old sha's thumbnail rows are gone, the old original file is gone, the new sha has the imported 500 thumbnail.
- [ ] **Step 3: Implement.** `Blob`; `Thumbnail` model re-keyed; service keyed by `(sha256, size)` with the failure cache a `HashSet<String>` of shas (comment: never pruned, bounded by distinct originals); `api/attachments.rs::thumb` loads the attachment, returns 404 when missing or not thumbnailable, calls `get_or_generate(&Blob::from(&att), size)`, ETag `"<sha>-<size>"` unchanged; `upload.rs::commit` inserts the 300 row by sha (`on_conflict_do_nothing` keeps a dedupe cheap); `import/run.rs` drops `invalidate_stale_thumbnails`, imports Homebox thumbnails by sha, and when an upsert changes an attachment's sha calls `remove_original` for the old sha after the transaction; `remove_original` deletes `thumbnails` rows for the sha under the same lock before removing the file. Update fixtures and existing tests.
- [ ] **Step 4: Verify; commit** `refactor: key thumbnails by content hash`.

### Task 2: AI settings service, environment overrides, GraphQL surface

**Files:**
- Create: `migrations/2026-09-26-110000_ai_settings/{up,down}.sql` (`INSERT OR IGNORE` the three defaults; down deletes `ai.%` rows), `src/svc/ai_settings.rs`, `src/ai/mod.rs`, `src/ai/env.rs`, `src/graphql/objects/ai_settings.rs`, `tests/graphql_ai_settings.rs`
- Modify: `src/svc/mod.rs`, `src/lib.rs`, `src/graphql/{context,query,mutation,inputs}.rs`, `src/graphql/objects/mod.rs`, `src/api/graphql.rs`, `src/routes.rs`, `src/main.rs`, `README.md`, `env.prod` (commented variables)

**Interfaces:**
- `ai::env::AiEnv { pub api_key: Option<String>, pub base_url: Option<String> }` with `AiEnv::from_env()` (reads `OPENAI_API_KEY`, `OPENAI_BASE_URL`, blank = unset) and `AiEnv::none()`.
- `svc::ai_settings`: `pub struct AiSettingsView { base_url, vision_model, synthesis_model, extra_instructions: Option<String>, has_api_key: bool, from_environment: Vec<&'static str> }`; `pub struct AiSettingsUpdate { base_url, vision_model, synthesis_model, extra_instructions: Option<String>, api_key: Option<String> }`; `pub struct AiConfig { base_url, api_key, vision_model, synthesis_model, extra_instructions: Option<String> }` (`Debug` redacts the key); `view(conn, env) -> Result<AiSettingsView>`; `update(conn, env, AiSettingsUpdate) -> Result<AiSettingsView>`; `config(conn, env) -> Result<Option<AiConfig>>` (`None` when no key anywhere); consts `BASE_URL_KEY` etc.; `normalise_base_url(&str) -> Result<String>` (trim, must be `http`/`https`, strip trailing slashes).
- `GraphQLContext` gains `pub ai: Arc<AiState>` where `ai::AiState { pub env: AiEnv, pub client: Arc<dyn AiClient> }`; `GraphQLContext::new(pool, actor, data_dir)` keeps its signature and defaults to `AiState::disabled()` (env none, a client whose `chat` returns `AiError::NotConfigured`); `with_ai(self, Arc<AiState>) -> Self`. The `AiClient` trait and `AiError` are defined in this task in `src/ai/client.rs` (trait, request/response types, error enum) so the context compiles; Task 3 adds the implementations.
- Router: `routes::app(pool, data_dir)` builds `AiState { env: AiEnv::from_env(), client: Arc::new(OpenAiClient::new()) }` (Task 3 supplies `OpenAiClient`; until then `app` uses the disabled client and Task 3 flips it); `app_with_thumbnails` and `app_with_actor` use `AiState::disabled()`; new `app_with_ai(pool, data_dir, Arc<AiState>)` for tests. `api::graphql::graphql_routes` receives `Arc<AiState>` as an Extension and passes it to the context.
- GraphQL (spec §6): `Query.aiSettings: AiSettings!`, `Mutation.updateAiSettings(input: AiSettingsInput!): AiSettings!`. `AiSettingsInput.apiKey`: omitted/null keep, `""` clear, otherwise replace (trimmed).

- [ ] **Step 1: Failing tests** in `tests/graphql_ai_settings.rs` (through the HTTP `/graphql` handler with `app_with_ai`): `defaults_are_reported_and_no_key_is_set`; `saving_a_key_is_write_only` (update with a key → response has `hasApiKey: true` and no field equal to the key anywhere in the JSON; `aiSettings` query likewise; the DB row holds it, read via `svc::ai_settings::config`); `an_empty_key_clears_it`; `a_null_key_keeps_it`; `env_key_wins_and_is_reported` (`AiEnv { api_key: Some("env-key"), .. }`: `fromEnvironment == ["OPENAI_API_KEY"]`, `hasApiKey` true, an update carrying `apiKey` is refused with a message containing `OPENAI_API_KEY`, an update without `apiKey` still saves the models); `base_url_is_normalised` (`https://host/v1/` → `https://host/v1`; `ftp://x` and `not a url` refused); `blank_models_are_refused`; `extra_instructions_over_4000_chars_are_refused` and blank becomes null; `read_only_actor_is_forbidden`. Unit tests in `svc::ai_settings` for `config()` resolution order (env over row) and the redacted `Debug`.
- [ ] **Step 2: Implement; verify; commit** `feat: AI settings with a write-only key and environment overrides`.

### Task 3: `AiClient`, `OpenAiClient`, `FakeAiClient`, prompts, `testAiConnection`

**Files:**
- Create: `src/ai/openai.rs`, `src/ai/fake.rs`, `src/ai/prompts.rs`, `tests/ai_client.rs`, `tests/support/openai_stub.rs`
- Modify: `Cargo.toml`/`Cargo.lock`, `src/ai/client.rs` (finalise types), `src/ai/mod.rs`, `src/svc/ai_settings.rs` (`test_connection`), `src/graphql/{mutation,objects/ai_settings}.rs`, `src/routes.rs` (real client in `app`), `tests/graphql_ai_settings.rs`, `tests/support/mod.rs`

**Interfaces:**
- `ai::client`: `ChatRequest { model: String, messages: Vec<Message>, response_format: Option<ResponseFormat>, max_tokens: Option<u32> }`; `Message { role: Role, content: Vec<ContentPart> }` (`Role::{System, User}`); `ContentPart::{Text(String), ImageUrl { url: String, detail: Detail }}` (`Detail::{Auto, Low, High}`); `ResponseFormat::{JsonSchema { name: String, schema: serde_json::Value }, JsonObject}`; `ChatResponse { content: String, usage: Option<Usage { prompt_tokens: u32, completion_tokens: u32 }>, model: String }`; `AiError::{NotConfigured, Transport(String), Timeout, Status { status: u16, snippet: String }, Malformed(String)}` (`Display` never includes the key; `snippet` is the body truncated to 500 chars with any occurrence of the key replaced by `***`); `#[async_trait] pub trait AiClient: Send + Sync { async fn chat(&self, config: &AiConfig, request: ChatRequest) -> Result<ChatResponse, AiError>; }`.
- `ai::openai::OpenAiClient::new() -> Self`; builders `with_timeout(Duration)`, `with_backoff(Vec<Duration>)` (default `[1s, 4s]`); request JSON: `{model, messages: [{role, content: [{type: "text", text} | {type: "image_url", image_url: {url, detail}}]}], response_format: {type: "json_schema", json_schema: {name, schema, strict: true}} | {type: "json_object"}, max_tokens}`; URL `format!("{}/chat/completions", config.base_url)`; header `Authorization: Bearer <key>`; parse `choices[0].message.content` as a string (a content-parts array is joined by its `text` parts); usage from `usage`; log `info!(model, latency_ms, prompt_tokens, completion_tokens)`.
- `ai::fake::FakeAiClient` (`pub`, not test-gated): `new()`, `push(Result<String, AiError>)`, `requests() -> Vec<ChatRequest>`; `chat` pops the queue (empty → `AiError::Malformed("fake queue empty")`).
- `ai::prompts`: `VISION_SYSTEM: &str`, `SYNTHESIS_SYSTEM: &str`, `photo_description_schema() -> Value` (spec §4.5), `item_suggestion_schema() -> Value` (spec §4.4), `with_extra_instructions(base: &str, extra: Option<&str>) -> String` (appends a final paragraph "Additional instructions from the user:\n…").
- `svc::ai_settings::test_connection(config: &AiConfig, client: &dyn AiClient) -> AiTestResult { ok: bool, message: String, latency_ms: i32 }` sends model = synthesis model, one user text "Reply with the single word OK.", `max_tokens: Some(8)`; ok when a response arrives, message = `"Connected: <model> answered in <n> ms"`, else the `AiError` display.
- GraphQL: `Mutation.testAiConnection: AiTestResult!` (async resolver; requires write; not configured → `{ok: false, message: "AI is not configured: add an API key first", latencyMs: 0}`).
- `tests/support/openai_stub.rs`: `OpenAiStub::start(handler) -> OpenAiStub { base_url }` runs an axum app on `127.0.0.1:0` capturing every request (`method, path, headers, json body`) and answering from a caller-supplied `Fn(usize /*call index*/, &Value) -> (StatusCode, Value)`.

- [ ] **Step 1: Failing tests** in `tests/ai_client.rs`: `sends_the_documented_request_shape` (auth header, path `/v1/chat/completions` for base `http://127.0.0.1:port/v1/`, model, one text part and one image part with detail auto, `response_format.type == "json_schema"` with `strict: true`); `falls_back_to_json_object_when_json_schema_is_rejected` (first call 400 `{"error":{"message":"Invalid parameter: 'response_format'..."}}`, second 200; exactly two calls, second has `json_object`); `retries_on_429_then_succeeds` (zero backoff; three calls); `gives_up_after_the_retries_on_500` (three calls → `AiError::Status{500,..}`); `does_not_retry_a_401` (one call; the error snippet, which echoes the header in the body, does not contain the key); `times_out` (`with_timeout(200ms)`, stub sleeps 1 s → `AiError::Timeout`); `parses_usage_and_content_parts_array`. In `tests/graphql_ai_settings.rs`: `test_connection_reports_ok_via_the_fake` and `test_connection_reports_not_configured`. Prompt unit tests: each system prompt names JSON-only output and the "never invent" rule; schemas list every field of spec §4.4/§4.5 as properties.
- [ ] **Step 2: Implement; verify (`cargo tree -i openssl` empty, `cargo build --release` links); commit** `feat: OpenAI-compatible AI client with fallback, retries and a connection test`.

### Task 4: Settings screen with the AI tab

**Files:**
- Create: `site/src/types/ai.ts`, `site/src/hooks/useAiSettings.ts`, `site/src/hooks/useAiSettingsMutations.ts`, `site/src/page/SettingsPage.tsx`, `site/src/page/settings/AiSettingsTab.tsx`, `site/src/page/settings/tabs.ts`, tests `site/src/page/__tests__/SettingsPage.test.tsx`, `site/src/page/__tests__/AiSettingsTab.test.tsx`, `site/src/hooks/__tests__/useAiSettingsMutations.test.tsx`
- Modify: `site/src/hooks/queries.ts`, `site/src/App.tsx` (`settings` → redirect to `settings/ai`; `settings/:tab`), `site/src/components/navLinks.ts` (add `{ to: '/settings', label: 'Settings' }`), `CLAUDE.md`

**Interfaces:**
- `types/ai.ts`: `AiSettings { baseUrl, visionModel, synthesisModel, extraInstructions: string | null, hasApiKey: boolean, fromEnvironment: string[] }`, `AiSettingsInput { baseUrl, visionModel, synthesisModel, extraInstructions: string | null, apiKey?: string | null }`, `AiTestResult { ok, message, latencyMs }`.
- `useAiSettings()` → `{ settings, loading, error }` (query `GetAiSettings`); `useUpdateAiSettings()` → `{ update(input), loading }` (refetch `['GetAiSettings']`, toast on failure, resolves the settings or null); `useTestAiConnection()` → `{ test(), loading }` resolving `AiTestResult | null`.
- `page/settings/tabs.ts`: `SETTINGS_TABS = [{ id: 'ai', label: 'AI' }] as const`.
- `SettingsPage`: reads `:tab`; unknown tab → `NotFound`; tab bar `role="tablist"` with `role="tab"` `aria-selected` links; renders the tab component.
- `AiSettingsTab`: form with `FormField`s: Base URL, Vision model, Synthesis model, Extra instructions (textarea), API key (`type="password"`, `autoComplete="off"`, empty; helper text "Key saved. Enter a new key to replace it." when `hasApiKey`, plus a "Clear key" button that sets a pending clear shown as "The key will be removed when you save"); fields listed in `fromEnvironment` are disabled with the note "Set from OPENAI_… in the environment"; Save sends `apiKey` as undefined (untouched), `""` (clear) or the typed value; "Test connection" button (disabled while `!hasApiKey`) showing the result inline with `text-success`/`text-danger`.

- [ ] **Step 1: Failing tests.** `SettingsPage`: `/settings` lands on the AI tab; `/settings/nope` shows NotFound; the tab bar marks AI selected. `AiSettingsTab`: prefilled from the query; the key input is empty even when `hasApiKey`; saving without touching the key sends no `apiKey`; typing a key sends it; "Clear key" then save sends `""`; env-provided fields are disabled with the note and the input sent omits nothing else; Test connection renders the ok message in `text-success` and a failure in `text-danger`; header nav shows Settings. Hook tests: `useUpdateAiSettings` refetches `GetAiSettings` (spied mock), `useTestAiConnection` resolves the result.
- [ ] **Step 2: Implement; verify (`yarn test --run && yarn lint && yarn build`); commit** `feat(site): settings screen with the AI tab`.

### Task 5: Acceptance and docs

- [ ] **Step 1:** Build the real bundle and release binary; import the real backup into a scratch dir under `/home/.build/cargo-target/`; serve on port 7054; confirm three imported thumbnails serve (200, webp) and that `sqlite3`/diesel shows `thumbnails` keyed by sha with the imported count; save AI settings through GraphQL with a dummy key, query them back and grep the response and the server log for the key (must be absent); run `testAiConnection` (expect a clean failure message with the dummy key against `https://api.openai.com/v1`, no key in the message; if `OPENAI_API_KEY` is present in the shell, also run once for real and record the latency); headless-Chrome screenshots of `/settings/ai` at desktop and phone width, read back. Stop the server, delete the scratch dir.
- [ ] **Step 2:** README (Settings screen, AI tab, env variables, "the SQLite file holds the API key: treat it as a secret"), CLAUDE.md (`ai/` module, `AiState` in the context, thumbnails by sha, settings keys; under 120 lines, no em dashes), spec §11 aligned with anything the implementation changed.
- [ ] **Step 3: Commit** `docs: AI settings, client and thumbnails by hash`.

---

## Self-review

- **Spec coverage (Phase 6 exit, spec §14):** thumbnails re-keyed with a migration and survival test (T1), `ai.*` settings with env overrides and a write-only key (T2), client + fake + prompts + `testAiConnection` (T3), settings screen with the AI tab and nav (T4), acceptance that a saved key is never shown again and existing thumbnails still serve (T5).
- **Type consistency:** `Blob`, `AiEnv`, `AiState`, `AiConfig`, `AiSettingsView/Update`, `AiClient`, `ChatRequest`, `AiError`, `AiTestResult` are spelled identically across tasks; `GraphQLContext::new` keeps its arity and gains `with_ai`.
- **Review Focus:** 1 → T1 migration and re-import tests; 2 → T2 write-only tests + T3 401 test; 3 → T2 env test; 4 → T3 request-shape test; 5 → T3 fallback and 429 tests.
