# Phase 3: Browsing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every imported location and item is reachable by clicking: a location tree in the sidebar, a location page with breadcrumbs, child locations and items, an item page with its details and photo, header search, and the HTTP endpoints that serve originals and on-demand multi-size thumbnails.

**Architecture:** Backend adds `Entity.parentId`, a flat `locations` query (the client builds the tree, since GraphQL cannot select recursively), retires `locationTree`, and adds two axum handlers: `/attachments/{id}` streams the content-addressed original, `/attachments/{id}/thumb/{size}` rounds the size to 300/500/1200, serves a cached WebP from the `thumbnails` table or generates it once (per-key async lock, `spawn_blocking`, EXIF orientation applied, quality 80). Attachment URLs carry `?v=<sha256 prefix>` so immutable caching stays correct across re-imports. Frontend adds hooks per query, a pure `buildLocationTree`, a recursive `LocationTree` in the sidebar (drawer on mobile), `Breadcrumbs`, `Thumb`, and three pages.

**Tech Stack:** as phases 1–2, plus `webp` 0.3 (libwebp via `cc`; the musl build image has a C toolchain), `image` 0.25 decode + `apply_orientation`, tokio `sync`/`spawn_blocking`, axum `Path` extraction; React Router 7 `useParams`/`Navigate`, Apollo `useQuery` with variables.

**Spec:** `docs/superpowers/specs/2026-09-25-home-tracker-design.md` (§6 with the phase-3 note, §7, §9, §11, §14, §15 "Phase 3").

## Global Constraints

- All phase-1/2 constraints hold: edition 2024, clippy `-D warnings`, fmt clean, OpenSSL-free (`cargo tree -i openssl` / `-i native-tls` empty; `webp` links libwebp via `libwebp-sys`, not TLS), `site/build/index.html` before cargo, `yarn` only, semantic colour classes only (the token guard test enforces it), hooks own all GraphQL, errors via `toast.error`, no em dashes in docs, commit trailer `Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF`.
- Thumbnail sizes: exactly `[300, 500, 1200]`. A requested size rounds **up** to the first allowed size ≥ it; above 1200 clamps to 1200; a non-numeric or non-positive size is HTTP 400. Non-image attachments are 404 on the thumb endpoint. Generation: decode with `image`, apply EXIF orientation, resize to fit inside `size × size` preserving aspect ratio, never upscale, encode WebP quality 80 with the `webp` crate. Store in `thumbnails (attachment_id, size)` with `mime_type = image/webp` and the produced width/height.
- Thumbnailable MIME types (case-insensitive): `image/jpeg`, `image/png`, `image/gif`, `image/webp`. One shared predicate `svc::thumbnail::is_thumbnailable(mime: &str) -> bool` is used by both the GraphQL `thumbnailUrl` and the HTTP handler.
- Attachment URLs: `Attachment.url = /attachments/{id}?v={first 12 hex of sha256}`, `Attachment.thumbnailUrl(size) = /attachments/{id}/thumb/{size}?v={same}`; the server ignores `v`. Responses carry `Cache-Control: public, max-age=31536000, immutable` and `ETag: "<sha256>"` (original) / `ETag: "<sha256>-<size>"` (thumb).
- Originals are streamed from `$DATA_DIR/originals/<sha256>` with the stored `mime_type` and `Content-Disposition: inline; filename="<title with quotes and control characters stripped>"`; a row whose file is missing on disk is 404 with a `warn!` log naming the id and path.
- `Query.locations: [Entity!]!` returns every location-type entity (archived included) ordered by `lower(name)`; `Entity.parentId: ID` is the raw column. `locationTree`, `svc::entity::location_tree`, `LocationNode` and the cycle-promotion code are removed in this phase; the client's `buildLocationTree` treats a location whose parent is not in the set as a root and, after the root walk, promotes any unvisited location (cycle members) to a root, sorted by name, so nothing is hidden.
- Routes: `/` (home: stats + root items + root locations), `/locations/:id`, `/items/:id`, `/search?q=`. Opening `/items/:id` for a location redirects to `/locations/:id` and vice versa; an unknown id renders a "Not found" state, never a crash.
- `routes::app(pool, data_dir: PathBuf) -> Router` (signature change; all callers updated).
- Steve's real backup remains the acceptance data: after import, every one of the 19 locations must appear in the tree and every item is reachable from its location page; the item page for an entity with a photo shows the thumbnail; the 1200 variant generates on demand.

## Review Focus

1. Two concurrent first requests for the same missing thumbnail must generate it once (per-key lock), and a request for a different size of the same attachment must not wait on the first. Task 3 test.
2. A thumbnail request for an attachment whose original file was deleted from disk: 404 with a log, not a 500 and not a panic inside `spawn_blocking`. Task 3 test.
3. An original whose stored `mime_type` is `image/JPEG` (uppercase) still gets a thumbnail URL and a generated thumbnail. Task 1 (predicate) and Task 3 (handler) tests.
4. A `title` containing `"` or a newline must not break the `Content-Disposition` header (header injection). Task 3 test.
5. The location tree UI with a location whose parent id points outside the location set (an item, or a deleted row) still shows that location as a root; a cycle shows its members as roots. Task 4 `buildLocationTree` tests.

---

### Task 1: `parentId`, flat `locations`, retire `locationTree`, shared MIME predicate, versioned URLs

**Files:**
- Create: `src/svc/thumbnail.rs` (predicate and size rounding only; generation arrives in Task 2)
- Modify: `src/svc/mod.rs`, `src/svc/entity.rs`, `src/graphql/objects/entity.rs`, `src/graphql/objects/attachment.rs`, `src/graphql/query.rs`, `tests/graphql_queries.rs`, `tests/import_end_to_end.rs`, `docs/superpowers/specs/2026-09-25-home-tracker-design.md` (§6 Query block)

**Interfaces:**
- Produces: `svc::entity::locations(conn) -> Result<Vec<Entity>>`; removes `svc::entity::{location_tree, LocationNode, take_subtree, take_lowest}` and their tests.
- Produces: `svc::thumbnail::{THUMB_SIZES: [u32; 3] = [300, 500, 1200], is_thumbnailable(mime: &str) -> bool, allowed_size(requested: i64) -> Option<u32>}` (`None` for non-positive; rounds up; clamps at 1200).
- Produces: `Attachment.url` and `thumbnailUrl` with `?v=` (helper `fn version_tag(sha256: &str) -> &str` = first 12 chars, or the whole string if shorter).
- Produces GraphQL: `Entity.parentId: ID`, `Query.locations`; removes `Query.locationTree` and the `LocationNode` type.

- [ ] **Step 1: Write the failing tests**

`src/svc/thumbnail.rs` (tests first, then the two functions):

```rust
//! Thumbnail policy: which MIME types can be thumbnailed and which sizes exist.

/// Allowed thumbnail box sizes, ascending. A request rounds up to the first
/// size that is at least as large; larger requests clamp to the last.
pub const THUMB_SIZES: [u32; 3] = [300, 500, 1200];

pub fn is_thumbnailable(mime: &str) -> bool {
    matches!(
        mime.trim().to_ascii_lowercase().as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    )
}

pub fn allowed_size(requested: i64) -> Option<u32> {
    if requested <= 0 {
        return None;
    }
    let wanted = u32::try_from(requested).unwrap_or(u32::MAX);
    Some(
        THUMB_SIZES
            .iter()
            .copied()
            .find(|s| *s >= wanted)
            .unwrap_or(THUMB_SIZES[THUMB_SIZES.len() - 1]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_up_and_clamps() {
        assert_eq!(allowed_size(1), Some(300));
        assert_eq!(allowed_size(300), Some(300));
        assert_eq!(allowed_size(301), Some(500));
        assert_eq!(allowed_size(500), Some(500));
        assert_eq!(allowed_size(501), Some(1200));
        assert_eq!(allowed_size(1200), Some(1200));
        assert_eq!(allowed_size(9999), Some(1200));
        assert_eq!(allowed_size(i64::MAX), Some(1200));
        assert_eq!(allowed_size(0), None);
        assert_eq!(allowed_size(-5), None);
    }

    #[test]
    fn mime_check_is_case_insensitive_and_closed() {
        // Review focus 3.
        for ok in ["image/jpeg", "image/JPEG", " Image/Png ", "image/gif", "image/webp"] {
            assert!(is_thumbnailable(ok), "{ok}");
        }
        for no in ["image/heic", "image/svg+xml", "image/avif", "application/pdf", ""] {
            assert!(!is_thumbnailable(no), "{no}");
        }
    }
}
```

`src/svc/entity.rs` test for `locations` (add to its tests module): `locations_lists_every_location_type_entity_sorted` → on `seed_sample`, names `["Attic", "Garage", "House", "Tote A"]` (archived Attic included, Tote type included). Delete `location_tree_*` tests and `ancestors` stays.

`tests/graphql_queries.rs`: replace `location_tree_nests_locations` with `locations_is_flat_with_parent_ids`: `{ locations { id name parentId isLocation } }` → four rows sorted by name; `Garage.parentId == "e-house"`, `House.parentId == null`, every `isLocation == true`. Replace `thumbnail_url_rounds_nothing_server_side_yet` with `attachment_urls_carry_a_version_tag`: `primaryPhoto { url thumbnailUrl(size: 300) }` → `url == "/attachments/a-drill-photo?v=aaaaaaaaaaaa"` (the fixture sha is `aa…`; use the first 12 chars of whatever the fixture stores), `thumbnailUrl == "/attachments/a-drill-photo/thumb/300?v=aaaaaaaaaaaa"`. Add `thumbnail_url_is_null_for_non_images_and_case_insensitive`: set the manual's mime to `application/PDF` and the photo's to `image/JPEG` via a direct update; `thumbnailUrl` null for the manual, present for the photo. Update the introspection test to drop `LocationNode`/`locationTree` and add `locations` + `parentId`.

`tests/import_end_to_end.rs`: replace the `locationTree` selection with `locations { name parentId }` and assert Garage (`parentId null`) and Tote 1 (`parentId == garage id`).

- [ ] **Step 2: Implement**

`svc::entity::locations`: `entities::table.inner_join(entity_types::table).filter(entity_types::is_location.eq(true)).order(sql("lower(entities.name)")).select(Entity::as_select()).load(conn)`. Remove the tree code. `Entity` object: `fn parent_id(&self) -> Option<ID>`. `Attachment` object: `url`/`thumbnail_url` use `version_tag(&self.sha256)` and `svc::thumbnail::is_thumbnailable(&self.mime_type)`. `Query`: add `locations`, remove `location_tree`. Spec §6: replace the `LocationNode` type and `locationTree` field with `locations: [Entity!]!` and `parentId: ID` on `Entity`, and drop the phase-3 note now that it is done.

- [ ] **Step 3: Verify and commit**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`

```bash
git add src tests docs/superpowers/specs
git commit -m "feat: flat locations query, parentId, versioned attachment urls

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 2: Thumbnail generation core

**Files:**
- Modify: `Cargo.toml` (`webp = "0.3"`), `src/svc/thumbnail.rs`

**Interfaces:**
- Produces: `svc::thumbnail::Generated { pub data: Vec<u8>, pub width: u32, pub height: u32 }`, `svc::thumbnail::generate_bytes(original: &[u8], size: u32) -> anyhow::Result<Generated>` (pure, blocking, no I/O), `svc::thumbnail::WEBP_QUALITY: f32 = 80.0`.

- [ ] **Step 1: Write the failing tests**

Add to `src/svc/thumbnail.rs` tests (helpers encode test images with the `image` crate; the EXIF helper splices an APP1 segment into a JPEG so the orientation path is exercised end to end):

```rust
    use image::{DynamicImage, ImageEncoder, RgbImage};
    use std::io::Cursor;

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, _| image::Rgb([(x % 256) as u8, 40, 200]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
            .write_image(&img, w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        out
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, image::Rgb([10, 200, 30]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    /// Inserts an APP1 Exif segment carrying only Orientation = `orientation`
    /// right after the SOI marker. Layout: FF E1, length, "Exif\0\0", TIFF header
    /// (little endian, IFD at offset 8), one IFD entry (tag 0x0112, SHORT, count 1).
    fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let mut tiff = vec![b'I', b'I', 0x2A, 0x00, 8, 0, 0, 0];
        tiff.extend_from_slice(&1u16.to_le_bytes()); // one entry
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]); // value padding to 4 bytes
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        let len = (payload.len() + 2) as u16;
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    fn decode(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory(bytes).unwrap()
    }

    #[test]
    fn fits_inside_the_box_preserving_aspect_ratio_and_encodes_webp() {
        let g = generate_bytes(&jpeg(1600, 1200), 500).unwrap();
        assert_eq!((g.width, g.height), (500, 375));
        assert_eq!(&g.data[..4], b"RIFF");
        assert_eq!(&g.data[8..12], b"WEBP");
        let decoded = decode(&g.data);
        assert_eq!((decoded.width(), decoded.height()), (500, 375));
    }

    #[test]
    fn portrait_images_are_bounded_by_height() {
        let g = generate_bytes(&jpeg(600, 1800), 300).unwrap();
        assert_eq!((g.width, g.height), (100, 300));
    }

    #[test]
    fn never_upscales() {
        let g = generate_bytes(&png(120, 80), 1200).unwrap();
        assert_eq!((g.width, g.height), (120, 80));
    }

    #[test]
    fn applies_exif_orientation() {
        // Orientation 6 = rotate 90° clockwise: a 400×200 source becomes 200×400.
        let rotated = with_exif_orientation(&jpeg(400, 200), 6);
        let g = generate_bytes(&rotated, 1200).unwrap();
        assert_eq!((g.width, g.height), (200, 400));
        // And without the tag the same bytes stay landscape.
        let plain = generate_bytes(&jpeg(400, 200), 1200).unwrap();
        assert_eq!((plain.width, plain.height), (400, 200));
    }

    #[test]
    fn rejects_garbage_and_non_images() {
        assert!(generate_bytes(b"not an image", 300).is_err());
        assert!(generate_bytes(b"%PDF-1.4 mini", 300).is_err());
    }
```

- [ ] **Step 2: Implement**

```rust
use std::io::Cursor;

use anyhow::{Context, Result};
use image::{DynamicImage, ImageDecoder, ImageReader};

pub const WEBP_QUALITY: f32 = 80.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decodes `original`, applies its EXIF orientation, fits it inside a
/// `size × size` box without upscaling, and encodes WebP at [`WEBP_QUALITY`].
/// Pure and blocking: call it from `spawn_blocking`.
pub fn generate_bytes(original: &[u8], size: u32) -> Result<Generated> {
    let mut decoder = ImageReader::new(Cursor::new(original))
        .with_guessed_format()
        .context("sniffing the image format")?
        .into_decoder()
        .context("opening the image")?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).context("decoding the image")?;
    img.apply_orientation(orientation);
    if img.width() > size || img.height() > size {
        img = img.resize(size, size, image::imageops::FilterType::Lanczos3);
    }
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let encoded = webp::Encoder::from_rgba(&rgba, width, height).encode(WEBP_QUALITY);
    Ok(Generated { data: encoded.to_vec(), width, height })
}
```

If `into_decoder()` is not available on `ImageReader` in 0.25.10, use `let decoder = reader.into_decoder()?` as `image::ImageReader::into_decoder` (it exists in 0.25; check `~/.cargo/registry/src/*/image-0.25.10/src/io/image_reader_type.rs`). `DynamicImage::resize` already preserves aspect ratio and fits inside the box.

- [ ] **Step 3: Verify and commit**

Run: `cargo test svc::thumbnail && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check && cargo tree -i openssl; cargo tree -i native-tls`

```bash
git add Cargo.toml Cargo.lock src/svc/thumbnail.rs
git commit -m "feat: thumbnail generation with orientation, fit-in-box and webp

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 3: Attachment HTTP handlers and the thumbnail service

**Files:**
- Create: `src/api/attachments.rs`, `src/svc/thumbnail_service.rs`, `tests/attachments.rs`
- Modify: `src/api/mod.rs`, `src/svc/mod.rs`, `src/routes.rs`, `src/main.rs`, `tests/graphql_summary.rs`, `tests/graphql_queries.rs`, `tests/spa_routes.rs`, `tests/import_end_to_end.rs` (all callers of `app`)

**Interfaces:**
- Produces: `svc::thumbnail_service::ThumbnailService::new(pool: SqlitePool, data_dir: PathBuf) -> Arc<Self>`, `async fn get_or_generate(&self, attachment_id: &str, size: u32) -> anyhow::Result<Option<Thumbnail>>` (`None` when the attachment does not exist, is not thumbnailable, or its original file is missing; `Err` only for unexpected failures), `pub fn generations(&self) -> u64` (counter for tests).
- Produces: `api::attachments::attachment_routes() -> Router` mounting `GET /attachments/{id}` and `GET /attachments/{id}/thumb/{size}`, reading `Extension<SqlitePool>`, `Extension<Arc<ThumbnailService>>`, `Extension<Arc<PathBuf>>` (data dir).
- Produces: `routes::app(pool: SqlitePool, data_dir: PathBuf) -> Router`.

- [ ] **Step 1: Write the failing integration tests**

`tests/attachments.rs` uses `TestDb`, `seed_sample` (whose photo has sha `aa…` and 3 bytes of content, not an image) plus a helper that writes a real JPEG (via the same encoder as Task 2's tests, copied into `tests/support/images.rs` and re-exported from `tests/support/mod.rs`) into `data_dir/originals/<sha>` and inserts an attachment row pointing at it. Tests:

- `serves_the_original_with_headers`: GET `/attachments/<id>?v=abc` → 200, body equals the file bytes, `content-type` = stored mime, `content-disposition` = `inline; filename="photo.jpg"`, `cache-control` immutable, `etag` = `"<sha256>"`.
- `strips_quotes_and_control_characters_from_the_filename` (Review Focus 4): title `evil".jpg\r\nX-Injected: 1` → header value is `inline; filename="evil.jpg X-Injected: 1"` (quotes removed, CR/LF replaced by a space) and no `x-injected` header exists.
- `unknown_attachment_is_404` and `missing_original_file_is_404_not_500` (delete the file; response 404).
- `thumb_rejects_bad_sizes`: `/thumb/abc`, `/thumb/0`, `/thumb/-3` → 400.
- `thumb_rounds_up_and_clamps`: `/thumb/1` and `/thumb/300` produce a 300 row; `/thumb/301` a 500 row; `/thumb/9999` a 1200 row; response `content-type: image/webp`, `etag` `"<sha256>-<size>"`, immutable cache; body decodes with `image` to the expected bounded dimensions.
- `thumb_is_404_for_non_images` (the manual, `application/pdf`) and `thumb_is_case_insensitive_on_mime` (Review Focus 3: mime `image/JPEG`).
- `thumb_is_generated_once_then_cached`: two sequential requests; `generations() == 1`; the `thumbnails` row count for the id is 1.
- `concurrent_misses_generate_once` (Review Focus 1): `tokio::join!` on eight simultaneous GETs for the same id/size → all 200, `generations() == 1`; then two different sizes concurrently → `generations() == 3` total, both rows exist.
- `thumb_for_a_missing_original_is_404` (Review Focus 2).
- `imported_homebox_thumbnail_is_served_without_generation`: insert a `thumbnails` row at 500 with arbitrary WebP-tagged bytes; GET `/thumb/500` returns exactly those bytes and `generations() == 0`.

Every existing `app(pool)` call in `tests/*.rs` becomes `app(pool, tmp_dir.path().to_path_buf())` (keep the `TempDir` alive in the test).

- [ ] **Step 2: Implement the service**

```rust
//! Serves cached thumbnails and generates missing ones exactly once per key.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use diesel::prelude::*;
use tokio::sync::Mutex;

use crate::db::SqlitePool;
use crate::models::{Attachment, Thumbnail};
use crate::svc::{attachment, thumbnail};

pub struct ThumbnailService {
    pool: SqlitePool,
    data_dir: PathBuf,
    locks: Mutex<HashMap<(String, u32), Arc<Mutex<()>>>>,
    generations: AtomicU64,
}

impl ThumbnailService {
    pub fn new(pool: SqlitePool, data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { pool, data_dir, locks: Mutex::new(HashMap::new()), generations: AtomicU64::new(0) })
    }

    pub fn generations(&self) -> u64 {
        self.generations.load(Ordering::Relaxed)
    }

    pub async fn get_or_generate(&self, attachment_id: &str, size: u32) -> Result<Option<Thumbnail>> {
        let Some(att) = self.load_attachment(attachment_id)? else { return Ok(None) };
        if !thumbnail::is_thumbnailable(&att.mime_type) {
            return Ok(None);
        }
        if let Some(hit) = self.cached(attachment_id, size)? {
            return Ok(Some(hit));
        }
        let key_lock = {
            let mut locks = self.locks.lock().await;
            locks.entry((attachment_id.to_owned(), size)).or_default().clone()
        };
        let _guard = key_lock.lock().await;
        if let Some(hit) = self.cached(attachment_id, size)? {
            return Ok(Some(hit)); // another request generated it while we waited
        }
        let path = self.data_dir.join("originals").join(&att.sha256);
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(id = attachment_id, path = %path.display(), "original file missing");
                return Ok(None);
            }
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        let generated = tokio::task::spawn_blocking(move || thumbnail::generate_bytes(&bytes, size))
            .await
            .context("thumbnail task panicked")??;
        self.generations.fetch_add(1, Ordering::Relaxed);
        let row = Thumbnail {
            attachment_id: attachment_id.to_owned(),
            size: size as i32,
            mime_type: "image/webp".to_owned(),
            width: generated.width as i32,
            height: generated.height as i32,
            data: generated.data,
            created_at: chrono::Utc::now().naive_utc(),
        };
        let mut conn = self.pool.get().context("db connection for thumbnail insert")?;
        diesel::insert_into(crate::schema::thumbnails::table)
            .values(&row)
            .on_conflict_do_nothing()
            .execute(&mut conn)?;
        {
            let mut locks = self.locks.lock().await;
            locks.remove(&(attachment_id.to_owned(), size));
        }
        Ok(Some(row))
    }

    fn load_attachment(&self, id: &str) -> Result<Option<Attachment>> {
        let mut conn = self.pool.get().context("db connection")?;
        attachment::get(&mut conn, id)
    }

    fn cached(&self, id: &str, size: u32) -> Result<Option<Thumbnail>> {
        let mut conn = self.pool.get().context("db connection")?;
        attachment::thumbnail(&mut conn, id, size as i32)
    }
}
```

(`spawn_blocking` returning `Result<Result<..>>` needs `??`; if `generate_bytes` fails, the error propagates as a 500 from the handler with the message logged, which is correct for a corrupt original.) Removing the map entry after generation keeps the map from growing; a waiter that already cloned the `Arc` still holds a valid lock.

- [ ] **Step 3: Implement the handlers**

`src/api/attachments.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Extension, Path};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio_util::io::ReaderStream;

use crate::api::AppError;
use crate::db::SqlitePool;
use crate::svc::attachment;
use crate::svc::thumbnail::allowed_size;
use crate::svc::thumbnail_service::ThumbnailService;

pub fn attachment_routes() -> Router {
    Router::new()
        .route("/attachments/{id}", get(original))
        .route("/attachments/{id}/thumb/{size}", get(thumb))
}

const IMMUTABLE: HeaderValue = HeaderValue::from_static("public, max-age=31536000, immutable");

/// `inline; filename="…"` with quotes, backslashes and control characters removed
/// so a hostile title cannot inject headers or break the quoting.
fn content_disposition(title: &str) -> HeaderValue {
    let clean: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|c| *c != '"' && *c != '\\')
        .collect();
    let name = if clean.trim().is_empty() { "attachment" } else { clean.trim() };
    HeaderValue::from_str(&format!("inline; filename=\"{name}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("inline"))
}

async fn original(
    Path(id): Path<String>,
    Extension(pool): Extension<SqlitePool>,
    Extension(data_dir): Extension<Arc<PathBuf>>,
) -> Result<Response, AppError> {
    let mut conn = pool.get()?;
    let Some(att) = attachment::get(&mut conn, &id)? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let path = data_dir.join("originals").join(&att.sha256);
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(%id, path = %path.display(), "original file missing");
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
        Err(e) => return Err(e.into()),
    };
    let body = Body::from_stream(ReaderStream::new(file));
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_str(&att.mime_type).unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))),
            (header::CONTENT_LENGTH, HeaderValue::from(att.size_bytes.max(0) as u64)),
            (header::CONTENT_DISPOSITION, content_disposition(&att.title)),
            (header::CACHE_CONTROL, IMMUTABLE),
            (header::ETAG, HeaderValue::from_str(&format!("\"{}\"", att.sha256)).expect("hex is a valid header")),
        ],
        body,
    )
        .into_response())
}

async fn thumb(
    Path((id, size)): Path<(String, String)>,
    Extension(service): Extension<Arc<ThumbnailService>>,
) -> Result<Response, AppError> {
    let Some(size) = size.trim().parse::<i64>().ok().and_then(allowed_size) else {
        return Ok((StatusCode::BAD_REQUEST, "size must be a positive integer").into_response());
    };
    let Some(row) = service.get_or_generate(&id, size).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let etag = {
        let mut conn = service_pool(&service).get()?; // see note below
        let sha = attachment::get(&mut conn, &id)?.map(|a| a.sha256).unwrap_or_default();
        HeaderValue::from_str(&format!("\"{sha}-{size}\"")).expect("valid header")
    };
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_str(&row.mime_type).unwrap_or_else(|_| HeaderValue::from_static("image/webp"))),
            (header::CACHE_CONTROL, IMMUTABLE),
            (header::ETAG, etag),
        ],
        row.data,
    )
        .into_response())
}
```

Simplify the ETag: make `get_or_generate` return `Option<(Attachment, Thumbnail)>` so the handler has the sha without a second lookup (drop the `service_pool` placeholder). Add `tokio-util = { version = "0.7", features = ["io"] }` to `Cargo.toml` for `ReaderStream`.

`src/routes.rs`: `pub fn app(pool: SqlitePool, data_dir: PathBuf) -> Router` builds `ThumbnailService::new(pool.clone(), data_dir.clone())`, merges `attachment_routes().layer(Extension(pool.clone())).layer(Extension(service)).layer(Extension(Arc::new(data_dir)))` before the GraphQL routes. `main.rs` passes `config.data_dir.clone()`.

- [ ] **Step 4: Verify and commit**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`. Then a manual check against the real backup: import it into a scratch dir under `/home/.build/cargo-target/`, run the server on port 7031, `curl -sI localhost:7031/attachments/4be28283-c79a-4300-99d0-f7e7492d2170` → 200 image/jpeg with the headers, `curl -s -o /tmp/t.webp -w '%{http_code} %{content_type}\n' localhost:7031/attachments/4be28283-c79a-4300-99d0-f7e7492d2170/thumb/1200` → `200 image/webp`, `file /tmp/t.webp` shows a WebP no larger than 1200 on the long edge; repeat the curl and confirm the server log shows no second generation. Kill the server, delete the scratch dir.

```bash
git add Cargo.toml Cargo.lock src tests
git commit -m "feat: serve originals and on-demand thumbnails over http

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 4: Frontend types, hooks, and the pure tree builder

**Files:**
- Create: `site/src/types/entity.ts`, `site/src/hooks/useLocations.ts`, `site/src/hooks/useEntity.ts`, `site/src/hooks/useSearch.ts`, `site/src/hooks/useRootItems.ts`, `site/src/utils/locationTree.ts`, `site/src/utils/__tests__/locationTree.test.ts`, `site/src/hooks/__tests__/useLocations.test.tsx`, `site/src/hooks/__tests__/useEntity.test.tsx`, `site/src/hooks/__tests__/useSearch.test.tsx`, `site/src/hooks/__tests__/useRootItems.test.tsx`
- Modify: `site/src/hooks/queries.ts`, `site/setupVitest.ts` (default mocks for the new operations so `App.test.tsx` keeps rendering)

**Interfaces:**
- Produces TS types: `EntityTypeRef { id; name; isLocation }`, `TagRef { id; name; color }`, `AttachmentRef { id; kind; primary; title; mimeType; url; thumbnailUrl: string | null }`, `EntityFieldRef { id; name; kind; textValue; numberValue; booleanValue; timeValue }`, `LocationSummary { id; name; parentId: string | null; archived }`, `EntityListItem { id; name; assetId; quantity; purchasePriceCents; archived; primaryPhoto: { thumbnailUrl } | null; entityType: EntityTypeRef }`, `EntityDetail` (every `Entity` field from spec §6 as camelCase, with `ancestors: LocationSummary[]`, `childLocations: EntityListItem[]`, `items: EntityListItem[]`, `tags`, `attachments`, `fields`, `parent: LocationSummary | null`).
- Produces GraphQL documents in `queries.ts`: `GET_LOCATIONS` (`locations { id name parentId archived }`), `GET_ENTITY` (`entity(id: $id) { …every detail field… }`), `SEARCH` (`search(query: $query, limit: 50) { …list fields… }`), `GET_ROOT_ITEMS`.
- Produces hooks: `useLocations(): { locations: LocationSummary[]; tree: LocationNode[]; loading; error }`, `useEntity(id: string): { entity: EntityDetail | null; loading; error; notFound: boolean }` (`notFound` = settled and `entity` null), `useSearch(query: string): { results: EntityListItem[]; loading; error }` (skips when `query.trim() === ''`), `useRootItems(): { items: EntityListItem[]; loading; error }`.
- Produces `utils/locationTree.ts`: `interface LocationNode { location: LocationSummary; children: LocationNode[] }`, `buildLocationTree(locations: LocationSummary[]): LocationNode[]`, `pathTo(tree: LocationNode[], id: string): LocationSummary[]` (root first; empty when absent).

- [ ] **Step 1: Write the failing tests**

`locationTree.test.ts` cases: nests by `parentId` and sorts siblings case-insensitively; a parent id not in the set makes a root (Review Focus 5); a two-node cycle makes both roots with no infinite loop and each id present exactly once; a self-parented node is a root; `pathTo` returns root-first ancestors including the node itself; `pathTo` of an unknown id is `[]`; an empty input gives `[]`.

Hook tests with `MockedProvider` (mirror `useSummary.test.tsx`): success returns data, error toasts the hook's message (`Error loading locations`, `Error loading item`, `Search failed`, `Error loading items`) and returns the empty value; `useEntity` sets `notFound` when the mock returns `entity: null`; `useSearch('')` issues no request (`skip`) and returns `[]`.

- [ ] **Step 2: Implement**

`buildLocationTree`: index by id; `children: Map<string | null, LocationSummary[]>` where a parent not in the set maps to `null`; walk from `null` with a `visited` set; afterwards, any location not visited is promoted to a root in name order and its subtree walked with the same visited set; sort every child list by `name.toLowerCase()`. `pathTo`: depth-first search returning the path.

Hooks: `useQuery` with `variables`, `skip` for empty search, `useErrorToast`. `setupVitest.ts`'s default `/graphql` mock must answer by operation name: parse `body.operationName` and return `summary`, `locations: []`, `rootItems: []`, `entity: null`, or `search: []` accordingly (keep the summary payload as is).

- [ ] **Step 3: Verify and commit**

Run in `site/`: `yarn test --run && yarn lint && yarn build`

```bash
git add site/src site/setupVitest.ts
git commit -m "feat(site): location, entity, search and root-item hooks with a pure tree builder

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 5: Sidebar tree, drawer, header search, breadcrumbs, thumbnail component

**Files:**
- Create: `site/src/components/LocationTree.tsx`, `site/src/components/Breadcrumbs.tsx`, `site/src/components/Thumb.tsx`, `site/src/components/SearchBox.tsx`, `site/src/components/__tests__/LocationTree.test.tsx`, `site/src/components/__tests__/Breadcrumbs.test.tsx`, `site/src/components/__tests__/Thumb.test.tsx`, `site/src/components/__tests__/SearchBox.test.tsx`
- Modify: `site/src/components/Sidebar.tsx`, `site/src/components/AppHeader.tsx`, `site/src/page/PageTemplate.tsx`

**Interfaces:**
- `<LocationTree nodes currentId expanded onToggle />` renders nested `<ul>`; each node is a `<Link to="/locations/{id}">` plus a chevron `<button aria-label="Expand X"/"Collapse X">` when it has children; the current node has `aria-current="page"` and the `bg-surface-raised` class; archived locations get `text-muted` and a "(archived)" suffix.
- `Sidebar` owns the expanded set: `useLocations()`, `useState` initialised from `localStorage` key `home-tracker.tree.expanded` (JSON array of ids, guarded by try/catch), auto-expands the ancestors of the current route's location (`pathTo`), persists on change. Below `md` it renders as a drawer: `PageTemplate` holds `drawerOpen` state, `AppHeader` gets a hamburger `<button aria-label="Open locations">` shown below `md`, the drawer is a fixed panel with a backdrop button `aria-label="Close locations"`; navigation closes it.
- `<Breadcrumbs trail: LocationSummary[] current?: string />`: `Home` link, then each ancestor as a link, then the current name as plain text, separated by `/` in `text-muted`.
- `<Thumb attachment: { thumbnailUrl: string | null; title? } size: 300 | 500 | 1200 className? />`: renders `<img src={thumbnailUrl with the size substituted} alt={title} loading="lazy">`; when `thumbnailUrl` is null renders a placeholder box with the `text-muted` "No photo" text. The size substitution replaces the `/thumb/<n>` segment; a helper `thumbUrlAt(url, size)` is exported and tested.
- `<SearchBox />` in the header: a form with `role="search"`, input `aria-label="Search items"`, submitting navigates to `/search?q=<encoded>`; Enter submits; empty input does nothing.

- [ ] **Step 1: Write the failing tests**

`LocationTree.test.tsx`: renders roots; a collapsed node hides its children and the chevron toggles them (`onToggle` called with the id); the current node has `aria-current="page"`; archived nodes show the suffix. `Breadcrumbs.test.tsx`: `Home / House / Garage` with links for the first two and text for the last. `Thumb.test.tsx`: `thumbUrlAt('/attachments/a/thumb/500?v=abc', 300) === '/attachments/a/thumb/300?v=abc'`; renders `img` with the right `src`/`alt`; renders the placeholder for null. `SearchBox.test.tsx` (wrap in a `MemoryRouter` with a route that renders `useLocation().search`): typing `drill` and pressing Enter navigates to `/search?q=drill`; whitespace-only does not navigate. Sidebar drawer: in `PageTemplate` test (new `page/__tests__/PageTemplate.test.tsx`), the hamburger opens the drawer (`Close locations` button appears) and the backdrop closes it.

- [ ] **Step 2: Implement, verify, commit**

Run in `site/`: `yarn test --run && yarn lint && yarn build`

```bash
git add site/src
git commit -m "feat(site): location tree sidebar with drawer, breadcrumbs, search box, thumbnails

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 6: Location, item and search pages, home page root items, routing

**Files:**
- Create: `site/src/page/LocationPage.tsx`, `site/src/page/ItemPage.tsx`, `site/src/page/SearchPage.tsx`, `site/src/page/NotFound.tsx`, `site/src/components/EntityList.tsx`, `site/src/components/LocationCards.tsx`, `site/src/components/DetailsGrid.tsx`, `site/src/components/TagChips.tsx`, `site/src/utils/date.ts`, tests for each page and component
- Modify: `site/src/App.tsx`, `site/src/page/HomePage.tsx`, `site/src/App.test.tsx`

**Interfaces:**
- Routes in `App.tsx`: `/`, `/locations/:id`, `/items/:id`, `/search`, `*` → `NotFound`.
- `LocationPage`: `useParams().id` → `useEntity(id)`; while loading show a skeleton; `notFound` → `<NotFound what="location" />`; if `entity.isLocation === false` → `<Navigate to={/items/${id}} replace />`; otherwise `Breadcrumbs trail=entity.ancestors current=entity.name`, `<h1>`, description, `<LocationCards locations=childLocations />` (cards linking to `/locations/:id`, count of items shown when available), `<EntityList items=items />` (rows: `Thumb` 300, name link to `/items/:id`, asset id in `text-muted`, quantity when ≠ 1, price via `formatCents` when > 0, archived badge), and an "Add item" button placeholder disabled with title "Coming in phase 4".
- `ItemPage`: symmetric redirect for locations; shows `Breadcrumbs` (ancestors), name, `Thumb` 1200 of `primaryPhoto` (falls back to the first `image/*` attachment), a `DetailsGrid` of the non-empty fields (asset id, type, quantity, manufacturer, model, serial, purchase date/from/price, warranty, sold, insured, notes, description), `TagChips`, custom `fields` list, other attachments as links to `url` (non-images by title), timestamps via `utils/date.ts` `formatDate(iso)` (`en-US`, short date), `formatDateTime`.
- `SearchPage`: reads `q` from `useSearchParams`, `useSearch(q)`, heading `Results for "q"`, `EntityList` with location rows linking to `/locations/:id` (use `entityType.isLocation`), empty state text.
- `HomePage` gains "Locations" (root locations as `LocationCards`, from `useLocations().tree` roots) and "Items without a location" (`useRootItems`) sections below the stats.
- `NotFound`: `<h1>Not found</h1>` and a link home.

- [ ] **Step 1: Write the failing tests**

Page tests wrap in `MockedProvider` + `MemoryRouter` with `initialEntries` and a `Routes` table mirroring `App.tsx` so redirects are observable: `LocationPage` renders breadcrumbs, child locations and items from a mock `entity`; redirects to `/items/:id` when the entity is an item; shows Not found on null. `ItemPage` mirror. `SearchPage` shows results for `q` and the empty state. `HomePage` shows root locations and root items in addition to the stats. `App.test.tsx` still renders the shell and home. `EntityList` shows quantity only when ≠ 1 and price only when > 0; `DetailsGrid` omits empty values; `formatDate('2021-10-23') === 'Oct 23, 2021'`.

- [ ] **Step 2: Implement, verify, commit**

Run in `site/`: `yarn test --run && yarn lint && yarn build`

```bash
git add site/src
git commit -m "feat(site): location, item and search pages with routing and root items on home

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 7: Acceptance against the real backup and docs

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `docs/superpowers/specs/2026-09-25-home-tracker-design.md` (§7 `?v=` note, §11 routes confirmed)

- [ ] **Step 1: End-to-end with the real data**

```bash
S=/home/.build/cargo-target/phase3-acceptance && rm -rf "$S" && mkdir -p "$S"
cd site && yarn build && cd ..
DATABASE_URL=$S/db.sqlite DATA_DIR=$S cargo run --release -- import homebox-backup
PORT=7033 DATABASE_URL=$S/db.sqlite DATA_DIR=$S /home/.build/cargo-target/release/home-tracker &
```

Then with headless Chrome (`google-chrome-stable --headless=new --no-sandbox --disable-gpu --window-size=1280,800 --virtual-time-budget=8000 --screenshot=<file> <url>`): screenshot `/` (stats, root locations, tree in the sidebar with all 8 roots), `/locations/<Garage id>` (breadcrumb `Home / Garage`, child locations, items with thumbnails), `/items/<an item id with a photo>` (large photo, details, tags). Also `curl -s localhost:7033/graphql -d '{"query":"{ locations { id } }"}' -H 'content-type: application/json' | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["data"]["locations"]))'` → 19. And confirm the thumbnails table grew: `sqlite3 $S/db.sqlite 'select size, count(*) from thumbnails group by size'` shows 500 (imported) plus 300 and 1200 rows generated by the pages. Save the three screenshots under `.superpowers/sdd/<plan workspace>/` and name them in the report. Kill the server; delete `$S`.

- [ ] **Step 2: Docs**

README: new "Browsing" paragraph (tree, location and item pages, search), the attachment endpoints and their cache behaviour (`?v=` versioning; thumbnails generated on demand at 300/500/1200 and cached in SQLite). CLAUDE.md: Architecture adds `api/attachments.rs`, `svc/thumbnail.rs`, `svc/thumbnail_service.rs`, the frontend pages/components/hooks; Key Conventions adds "attachment URLs carry `?v=<sha256 prefix>` so immutable caching survives re-imports; `is_thumbnailable` is the single MIME gate; the tree is built client-side from the flat `locations` query". Spec §7: add the `?v=` sentence and `ETag` values. Under 120 lines, no em dashes.

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md docs/superpowers/specs
git commit -m "docs: browsing, attachment endpoints and thumbnail caching

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

## Self-review

- **Spec coverage (Phase 3 exit criteria, §15):** sidebar tree (T5), location page (T6), item page read-only (T6), breadcrumbs (T5/T6), search (T5/T6), `Thumb` backed by the thumbnail endpoint (T5 + T3), original serving and on-demand generation (T2/T3), every imported entity reachable (T7 acceptance). Phase-2 carry-overs closed here: shared case-insensitive `is_thumbnailable` (T1/T3), `thumbnailUrl` non-positive size → handler 400 (T3), tree shape decision and `LocationNode` removal from `svc` (T1).
- **Placeholders:** none; described components carry their props, behaviour and test names.
- **Type consistency:** `allowed_size`, `is_thumbnailable`, `generate_bytes`/`Generated`, `ThumbnailService::{new, get_or_generate, generations}`, `app(pool, data_dir)`, `LocationSummary`/`LocationNode`/`buildLocationTree`/`pathTo`, `thumbUrlAt`, hook return shapes are spelled identically across tasks.
- **Review Focus:** 1 → T3 `concurrent_misses_generate_once`; 2 → T3 `thumb_for_a_missing_original_is_404`; 3 → T1 predicate test + T3 `thumb_is_case_insensitive_on_mime`; 4 → T3 `strips_quotes_and_control_characters_from_the_filename`; 5 → T4 `locationTree.test.ts`.
