# Phase 5: Photos Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A photo uploaded from the browser is stored content-addressed, appears on the item page at all three thumbnail sizes, can be made primary or deleted, and the whole attachment surface is hardened for user-supplied files.

**Architecture:** A multipart `POST /api/upload/{entityId}` handler streams the `file` field to a temp file under `originals/` while hashing, sniffs the real image format from magic bytes (the client's content type is advisory), verifies the header decodes, renames the file into place (or drops it when the hash already exists), inserts the attachment row (first photo becomes primary, or `primary=true` takes over), and returns the attachment as JSON in the same shape GraphQL uses. An `Actor` request-extension middleware becomes the single auth seam for GraphQL and the file handlers (spec §10). The thumbnail service gains a negative cache for undecodable originals. Frontend: `useUploadPhoto` (plain `fetch` + `FormData`, then Apollo refetch/eviction), a `PhotoUploader`, a `PhotoGallery` with a `Lightbox` on the entity pages.

**Tech Stack:** as before, plus axum `multipart` feature (multer), tower-http `limit` feature (`RequestBodyLimitLayer`), `tokio::fs`, `sha2` streaming, `image::ImageReader::into_dimensions` for header validation; React file input + `fetch`.

**Spec:** `docs/superpowers/specs/2026-09-25-home-tracker-design.md` (§7 `/api/upload`, §9 "Uploads", §10, §11, §14, §15 "Phase 5").

## Global Constraints

- All prior constraints hold (edition 2024, clippy `-D warnings`, fmt, OpenSSL-free, `site/build/index.html` before cargo, `yarn` only, semantic colour classes, hooks own GraphQL, `toast.error`, `ConfirmDialog` for deletes, no em dashes in docs, commit trailer `Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF`).
- Upload contract (spec §7): `POST /api/upload/{entityId}`, multipart field `file` (required, at most one per request), optional field `primary` (`true`/`false`). Responses: `201` with JSON `{ id, kind, primary, title, mimeType, sizeBytes, url, thumbnailUrl }`; `400` missing/duplicate `file` field or bad multipart; `403` when the actor cannot write; `404` unknown entity; `413` body over `MAX_UPLOAD_BYTES = 25 MiB` (set by `RequestBodyLimitLayer` on the upload route only); `415` when the sniffed format is not JPEG/PNG/GIF/WebP (magic bytes decide, never the client's content type or filename); `422` when the header does not decode (`into_dimensions` fails) or the dimensions exceed the phase-4 `image::Limits`.
- Storage (spec §9): stream to `originals/.upload-<uuid v7>` while hashing; on success rename to `originals/<sha256>` if absent, else delete the temp file (dedupe); on any failure delete the temp file. `title` = the client filename with path components and control characters stripped, defaulting to `photo.<ext>`; `mime_type` = the sniffed type; `kind = photo`; `size_bytes` from the byte count.
- Primary rule: the entity's first photo becomes primary; `primary=true` on a later upload takes over and exactly one photo of the entity is primary afterwards (reuse `svc::attachment::set_primary`).
- Auth seam (spec §10): a router-level `middleware::from_fn` inserts `Extension(Actor::Anonymous)` on every request; the GraphQL handler and the upload handler read `Extension<Actor>` instead of constructing it. When auth arrives only the middleware changes.
- Thumbnail hardening: an original that fails to decode is remembered in `ThumbnailService.failed: Mutex<HashSet<String>>` keyed by attachment id for the process lifetime; requests then 404 with one `warn!` (no repeated decoding); a successful re-upload under the same id is impossible (ids are new), so no invalidation is needed. 500 bodies never include filesystem paths (`AppError` prints the outermost message only; the read error's context must not carry the path; log it instead).
- Frontend: `useUploadPhoto(entity: { id, parentId })` returns `{ upload(files: File[], options?: { primary?: boolean }): Promise<UploadResult[]>, uploading, progress }` using `fetch` (not Apollo); per-file result `{ file, ok, error? }`; after any success it refetches `GetEntity`, `GetSummary`, `GetRootItems`, `GetLocations` and evicts the entity. Errors map status codes to messages (`413` → "File is larger than 25 MB", `415` → "Only JPEG, PNG, GIF and WebP images are supported", `422` → "That file is not a readable image").
- `PhotoUploader`: `<input type="file" accept="image/jpeg,image/png,image/gif,image/webp" multiple>` behind a `Add photos` button, per-file status list, drag-and-drop onto the gallery region (optional but included: `onDrop` with `dataTransfer.files`). `PhotoGallery`: grid of `Thumb` 300 with a `Primary` badge, `Make primary` and `Delete` (via `ConfirmDialog`) per photo, click opens `Lightbox` (role `dialog`, 1200 variant, `Previous`/`Next` buttons, Escape closes, focus returns). Both entity pages (item and location) get the gallery and uploader; the item page's existing hero stays.
- Phase-4 final-review carry-overs this phase MUST close: (a) an upload of identical bytes racing an attachment delete can lose the file (delete counts sharers, then removes after commit; upload renames in, then inserts): serialise per sha with a keyed lock like the thumbnail lease, or re-check the sharer count under that lock before removing; (b) deleting the primary photo promotes the earliest remaining photo to primary (consistent with "first photo becomes primary"), with a test; (c) `useUploadPhoto` uses `fetch`, so export the refetch/evict policy from `useRefetchingMutation` (a `refetchAfterWrite(client, ids)` helper) and use it there. (d) imported tag colours bypass the server hex check: render the tags-page swatch only for a valid hex value (client guard) and normalise colours on import (invalid → NULL with a warning).
- Carry-overs this phase MUST close: corrupt-original negative cache and path scrubbing (above); `ThumbSize` newtype for `allowed_size`/`get_or_generate`; a test asserting the missing-original `warn!` (use `tracing-test` or a capture layer); phase-4 final-review items the controller triages into this phase (see its ledger).

## Review Focus

1. A file named `photo.jpg` with content type `image/jpeg` whose bytes are HTML: `415`, nothing stored, no temp file left. Task 1 test.
2. Uploading identical bytes twice to two entities: two rows, one file; deleting one keeps the file, deleting both removes it. Task 1 test.
3. A body over the limit: `413`, a JSON error body, and `originals/` has no `.upload-*` leftovers. Task 2 test.
4. A read-only actor uploading: `403` before the body is consumed (assert with a large body that the handler returns quickly and nothing is written). Task 2 test.
5. Primary semantics: first upload primary; second with `primary=true` takes over; `primaryPhoto` in GraphQL agrees; exactly one `is_primary`. Task 1 test.

---

### Task 1: Upload storage service, format sniffing, primary rule, thumbnail hardening

**Files:**
- Create: `src/svc/upload.rs`, `src/svc/sniff.rs`
- Modify: `Cargo.toml` (axum `multipart`, tower-http `limit`; `tracing-test` dev-dep), `src/svc/mod.rs`, `src/svc/attachment.rs`, `src/svc/thumbnail.rs` (`ThumbSize`), `src/svc/thumbnail_service.rs` (negative cache, path scrub, `ThumbSize`), `src/api/attachments.rs` (use `ThumbSize`), `tests/attachments.rs`

**Interfaces:**
- `svc::sniff::{ImageFormat { Jpeg, Png, Gif, WebP }, sniff(head: &[u8]) -> Option<ImageFormat>, ImageFormat::{mime(self) -> &'static str, extension(self) -> &'static str}}` (JPEG `FF D8 FF`, PNG 8-byte signature, GIF `GIF87a`/`GIF89a`, WebP `RIFF????WEBP`).
- `svc::upload::{TempUpload::create(originals_dir) -> Result<TempUpload>` (opens `originals/.upload-<uuid>`, hashes while `write_all`), `TempUpload::write(&mut self, chunk: &[u8]) -> Result<()>`, `TempUpload::finish(self) -> Result<Staged { path, sha256, size_bytes, head: [u8; 16] }>`, `Drop` removes the temp file if not finished; `StoredUpload { attachment: Attachment }`; `store(conn, data_dir, entity_id, staged: Staged, title: &str, primary: bool) -> Result<StoredUpload>` which sniffs, validates `into_dimensions` with the phase-4 limits, renames or dedupes, inserts the row with the primary rule, and on any error removes the staged file when no row references its hash. `clean_title(raw: Option<&str>, ext: &str) -> String`. `UploadError { NotFound, Unsupported, Unreadable, Io(anyhow) }` with `thiserror`.
- `svc::thumbnail::ThumbSize(u32)` returned by `allowed_size`, consumed by `get_or_generate(&self, id, ThumbSize)`.
- `ThumbnailService::get_or_generate` returns `Ok(None)` after a `warn!` for an undecodable original and remembers the id.

- [ ] **Step 1: Failing tests** — `svc::sniff` unit tests for the four formats plus HTML/PDF/empty → `None`; `svc::upload` tests on `TestDb` + temp dir: `stores_a_jpeg_and_makes_the_first_photo_primary`, `second_upload_with_primary_true_takes_over_and_exactly_one_is_primary` (Review Focus 5, includes `svc::attachment::primary_photo`), `identical_bytes_dedupe_to_one_file_with_two_rows_and_refcounted_delete` (Review Focus 2), `html_disguised_as_jpeg_is_unsupported_and_leaves_no_file` (Review Focus 1), `truncated_png_header_is_unreadable`, `unknown_entity_is_not_found`, `temp_file_is_removed_when_not_finished`, `clean_title_strips_paths_and_controls`; `tests/attachments.rs`: `undecodable_original_is_404_once_then_cached` (write garbage under a sha, GET thumb twice, `generations()` unchanged, both 404), `missing_original_logs_a_warning` (tracing capture), `thumb_size_newtype_is_used_end_to_end` (type-level: `allowed_size(301) == Some(ThumbSize(500))`).
- [ ] **Step 2: Implement; verify (`cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`; `cargo tree -i openssl` empty); commit** `feat: upload storage with format sniffing, primary rule and thumbnail hardening`.

---

### Task 2: Actor middleware, upload handler, body limit, read-only refusal

**Files:**
- Create: `src/api/upload.rs`, `src/api/actor.rs`, `tests/upload.rs`
- Modify: `src/api/mod.rs`, `src/api/graphql.rs` (read `Extension<Actor>`), `src/routes.rs`, `src/api/attachments.rs` (no behaviour change; reads `Extension<Actor>` only if a later phase needs it, leave a comment), `site/vite.config.ts` proxy already covers `/api/`.

**Interfaces:**
- `api::actor::attach_actor` middleware (`middleware::from_fn`) inserting `Actor::Anonymous`; `routes::app` applies it at the top level so every route sees it; a test helper `app_with_actor(pool, data_dir, actor)` for tests that need `Role::ReadOnly`.
- `api::upload::upload_routes() -> Router` with `POST /api/upload/{entity_id}` wrapped in `RequestBodyLimitLayer::new(MAX_UPLOAD_BYTES)`; handler: check `actor.can_write()` → `403` JSON before touching the multipart stream; iterate fields; `file` streamed chunk-by-chunk into `TempUpload`; `primary` parsed leniently; `UploadError` → status mapping; success `201` with the JSON shape in Global Constraints (`url`/`thumbnailUrl` built exactly as the GraphQL `Attachment` object builds them: extract a shared `attachment_urls(&Attachment) -> (String, Option<String>)` into `svc::attachment` and use it from both places, with a test that the two agree).
- Errors as JSON `{ "error": "<message>" }` with the mapped status; multer's `413` from the limit layer is mapped to the same JSON shape via a fallback in the handler (`axum::extract::multipart::MultipartError` with `status() == 413`) or by a `HandleErrorLayer`; whichever is used, the test asserts the JSON body.

- [ ] **Step 1: Failing tests** in `tests/upload.rs` (axum-test with `multipart` helpers): `uploads_a_jpeg_and_returns_the_attachment_json_with_thumbnails_at_all_sizes` (then GET `/attachments/{id}/thumb/300|500|1200` → 200 webp each), `first_photo_is_primary_and_graphql_agrees`, `primary_true_takes_over`, `rejects_html_as_415_and_stores_nothing` (Review Focus 1), `over_limit_is_413_json_with_no_temp_left` (Review Focus 3: 26 MiB body), `read_only_actor_is_403_before_the_body_is_read` (Review Focus 4: `app_with_actor(.., ReadOnly)`, 5 MiB body, assert `originals/` unchanged and response time < 2 s), `missing_file_field_is_400`, `unknown_entity_is_404`, `graphql_handler_reads_the_actor_extension` (mutation via the HTTP handler with the read-only app → `Forbidden`), `urls_in_json_match_graphql` (compare with `entity { attachments { url thumbnailUrl } }`).
- [ ] **Step 2: Implement; verify; commit** `feat: multipart photo upload behind the actor seam with a body limit`.

---

### Task 3: `useUploadPhoto`, `PhotoUploader`, `PhotoGallery`, `Lightbox`

**Files:**
- Create: `site/src/hooks/useUploadPhoto.ts`, `site/src/components/PhotoUploader.tsx`, `site/src/components/PhotoGallery.tsx`, `site/src/components/Lightbox.tsx`, `site/src/utils/uploadErrors.ts`, tests for each
- Modify: `site/src/page/ItemPage.tsx` and `site/src/page/LocationPage.tsx` (gallery + uploader sections, hero unchanged), `site/setupVitest.ts` (`fetch` mock for `/api/upload/`)

- [ ] **Step 1: Failing tests** — hook: posts `FormData` with `file` and optional `primary`, maps each status to the Global Constraints message, refetches/evicts on success (cache-state assertion as in phase 4), reports `progress` per file; `PhotoUploader`: selecting two files calls `upload` once with both, shows per-file status and errors, drag-and-drop calls `upload`; `PhotoGallery`: renders `Thumb` 300 per photo with the `Primary` badge, `Make primary` calls `setPrimary`, `Delete` opens `ConfirmDialog` then calls `remove`, clicking a thumb opens `Lightbox` at that index; `Lightbox`: 1200 `src`, `Previous`/`Next` wrap or disable at ends, Escape closes and focus returns to the clicked thumb; pages: gallery and uploader present, hero unchanged.
- [ ] **Step 2: Implement; verify (`yarn test --run && yarn lint && yarn build`); commit** `feat(site): photo upload, gallery and lightbox on entity pages`.

---

### Task 4: Acceptance and docs

- [ ] **Step 1:** Import the real backup into a scratch dir under `/home/.build/cargo-target/`; run the release binary on port 7053; with `curl -F file=@<a JPEG from homebox-backup/attachments/<photo id>> -F primary=true http://localhost:7053/api/upload/<Office item id>` confirm `201`, then GET the three thumb sizes (`200 image/webp`, correct bounded dimensions via `file`); upload the same bytes to a second item and confirm `ls originals | wc -l` did not grow; upload a text file renamed `.jpg` → `415`; a 30 MB file → `413`. Headless-Chrome screenshots of the item page gallery and (via `?lightbox=<id>` is not required; instead screenshot the gallery only) at desktop and phone widths; read them back. Kill the server, delete the scratch dir.
- [ ] **Step 2:** README (uploading photos, limits, formats), CLAUDE.md (upload route, actor middleware seam, sniffing rule, negative cache), spec §7/§9/§10 aligned with the implemented status codes and the middleware. Under 120 lines, no em dashes.
- [ ] **Step 3: Commit** `docs: photo uploads, actor seam and attachment hardening`.

---

## Self-review

- **Spec coverage (Phase 5 exit criteria, §15):** upload endpoint (T2) on a hardened storage service (T1), `useUploadPhoto` + gallery + primary selection + delete (T3), acceptance that a browser-uploaded photo appears at all three sizes (T2 test + T4). Auth seam per §10 (T2). Carry-overs closed (T1, T3).
- **Type consistency:** `TempUpload`/`Staged`/`store`/`UploadError`, `ThumbSize`, `attachment_urls`, `useUploadPhoto`'s return shape and the status→message map are spelled identically across tasks.
- **Review Focus:** 1 → T1 `html_disguised_as_jpeg_…` + T2 `rejects_html_as_415…`; 2 → T1 dedupe test; 3 → T2 `over_limit_is_413…`; 4 → T2 `read_only_actor_is_403…`; 5 → T1 + T2 primary tests.
