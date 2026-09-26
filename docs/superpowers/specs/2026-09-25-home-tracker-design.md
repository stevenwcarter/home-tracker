# home-tracker design

**Date:** 2026-09-25
**Status:** approved (brainstorm with Steve, `/ship-it --ask`)
**Scope:** the whole v1 program. Each phase gets its own implementation plan under
`docs/superpowers/plans/`; a phase may add a short addendum spec if its learnings
change something here.

## 1. Purpose

A self-hosted home inventory that replaces Homebox for Steve's household. v1 must:

- import a Homebox backup zip (the current "entities" schema, `schemaVersion: 1`)
  without losing data;
- model the same asset types and fields Homebox has, and let the user add more
  types and mark any of them as **locations** (a type whose entities can hold
  other entities in the tree);
- let the user browse locations as a tree, open a location, and add items to it;
- show the Homebox "quick statistics" on the home page: Total Value, Total Items,
  Total Locations, Total Tags;
- be shaped so that authentication can be added later without a data migration
  of the inventory tables.

Success for v1 is: Steve's real backup imports cleanly, every location and item
from it is reachable through the tree, new items can be added with a photo, and
the home page numbers match what Homebox shows for the same data.

## 2. Non-goals for v1

- Authentication, users, invitations, API keys.
- Homebox "tools": label/QR generation, CSV import/export, maintenance
  scheduling, notifiers, product lookup, AR scanner, templates UI.
- Multiple homes/groups per instance. One instance is one home.
- Light theme beyond a stub. Dark is the default and the only finished theme.
- Storybook.
- Web-upload of a backup zip (CLI import only; the web path is a later phase).
- Pre-generating thumbnails (they are generated on demand; a scheduled
  pregeneration job is a later addition).

## 3. Decisions

Answers from the brainstorm. "(H)" means it matches Homebox's own behaviour as
read from its source at `homebox/` (commit `08bb4665`, 2026-09-23).

| # | Topic | Decision |
|---|-------|----------|
| 1 | Item nesting | Any entity may have any parent (H). Only location-type entities appear in the tree navigation. |
| 2 | Fields on locations | Every entity has every field (H). The UI collapses purchase/warranty/sold sections by default for location types. |
| 3 | Custom fields, templates | Tables exist and the importer fills them. Custom fields render read-only on the item page. No template UI. |
| 4 | Maintenance, notifiers | Not imported. The importer logs a warning with the row count if present. |
| 5 | Deleting a location with children | Refused until the children are moved. |
| 6 | Type definition | name, description, icon, `is_location`. No per-type field templates in v1. |
| 7 | Total Value | `SUM(purchase_price × quantity)` over non-archived, non-location entities. Sold items are included (H). |
| 8 | Counts | Items = non-location, non-archived (H). Locations = location-type, archived included (H). Tags = all tags (H). |
| 9 | Money | Integer minor units in the database (`*_cents`). Currency code is a settings row, default `USD`. |
| 10 | Import trigger | CLI subcommand `home-tracker import <zip-or-dir>`. |
| 11 | Import semantics | Idempotent upsert keyed on the Homebox UUIDs, which become our primary keys. Re-running updates rows and never deletes. |
| 12 | Thumbnails on import | Homebox's WebP thumbnails are stored as the 500 variant. Other sizes are generated lazily. |
| 13 | Asset IDs | Kept. Displayed as `000-007` (`%06d` split 3/3, zero means none) (H). New entities get `max+1`. |
| 14 | `homebox-backup/`, `homebox/` | Excluded via `.git/info/exclude`. Importer tests use a small synthetic fixture. |
| 15 | Originals on disk | Content-addressed: `$DATA_DIR/originals/<sha256-hex>`. |
| 16 | Thumbnails | Allowed sizes 300, 500, 1200. Path parameter. Generated on request, cached in SQLite. Fit inside a size×size box, WebP quality 80 (H for 500). |
| 17 | Non-photo attachments | Imported and served. Upload in v1 is photos only. |
| 18 | Port | 7008. Docker maps it, so the number is not important. |
| 19 | Navigation | Persistent left sidebar with the expandable location tree. Main pane: breadcrumb, child locations, items. Drawer on mobile. |
| 20 | Location page scope | Direct children only. "Include nested" is a later toggle. |
| 21 | Editing | Item detail page with an edit form shared with the create form, including a parent picker (move). |
| 22 | Tags | Imported, shown as chips, assignable and creatable from the entity form. |
| 23 | Search | Name search box in the header. |
| 24 | Photo upload | Yes, on the item page, last phase. |
| 25 | GraphQL types in TS | Hand-written, as in chore-tracker. |
| 26 | Theming | Tailwind v4, semantic tokens as CSS variables under `data-theme` on `<html>`, dark default, light stubbed. |
| 27 | Storybook | No. |
| 28 | Layout | Single crate at the repo root plus `site/`. Importer is a module. |
| 29 | CI | Same jobs as chore-tracker: tarpaulin coverage, site build, Docker build/push over Tailscale, Watchtower trigger. Image `home-tracker`. Same secret names. |
| 30 | Remote | `origin` already points at `github.com:stevenwcarter/home-tracker`. |
| 31 | Ops | `healthcheck` subcommand, jemalloc on musl, `DATA_DIR`. |
| 32 | Phases | Five, see §14. |
| 33 | Phase-boundary questions | Post in chat with the default, wait ten minutes, then proceed on the default. |

## 4. Architecture

```
home-tracker/
├── Cargo.toml              single crate, edition 2024, default-run home-tracker
├── rustfmt.toml            edition = "2024"
├── diesel.toml
├── migrations/             diesel migrations, embedded at build time
├── src/
│   ├── main.rs             clap: `serve` (default), `import`, `healthcheck`
│   ├── lib.rs              re-exports for tests
│   ├── config.rs           env → Config (PORT, LISTEN_ADDRESS, DATABASE_URL, DATA_DIR, RUST_LOG)
│   ├── net.rs              dual-stack bind (port of chore-tracker's)
│   ├── db.rs               r2d2 pool, PRAGMA customizer, embedded migrations
│   ├── schema.rs           diesel print-schema output (generated)
│   ├── models/             diesel Queryable/Insertable structs, one file per table
│   ├── svc/                business logic, one file per aggregate (entity, entity_type, tag, attachment, thumbnail, stats, settings)
│   ├── graphql/            juniper: context.rs, schema.rs (RootNode), query.rs, mutation.rs, objects/ (one file per GraphQL object), inputs.rs
│   ├── api/                axum handlers: graphql.rs, attachments.rs, upload.rs
│   ├── routes.rs           router, compression, static assets via rust-embed
│   ├── import/             homebox backup reader: zip.rs, manifest.rs, tables.rs (serde structs), run.rs (upsert order)
│   ├── healthcheck.rs
│   └── money.rs, asset_id.rs   small value types with unit tests
├── site/                   Vite + React 19 + TypeScript + Apollo + Tailwind v4
│   └── src/
│       ├── App.tsx         ApolloProvider, router, lazy pages
│       ├── theme/          tokens.css, ThemeProvider, useTheme
│       ├── hooks/          one hook per GraphQL operation, queries.ts
│       ├── components/
│       ├── page/
│       └── types/
├── tests/                  Rust integration tests (axum-test against an in-memory app)
├── Dockerfile, .dockerignore, env.prod, justfile, .husky/, package.json
└── .github/workflows/rust.yml
```

**Backend request flow.** Axum receives `/graphql` → juniper executes against
`RootNode<Query, Mutation, EmptySubscription>` with a `GraphQLContext { pool,
data_dir, actor, thumbnails }` → resolvers call `svc::*` functions that take a
`&mut SqliteConnection` and return `anyhow::Result` → errors become GraphQL
errors through one translation helper. Attachment bytes bypass GraphQL and are
served by plain axum handlers.

**Frontend data flow.** Every GraphQL operation lives in exactly one hook under
`site/src/hooks/` (Apollo `useQuery`/`useMutation`), returning plain data plus
loading/error. Pages compose hooks and components; components never call Apollo
directly. Errors surface via `toast.error`.

## 5. Data model

All ids are UUID strings (`TEXT`), preserved from Homebox on import and `uuid::v7`
for new rows. Timestamps are `TIMESTAMP` (chrono `NaiveDateTime`, UTC). Civil
dates (purchase, sold, warranty) are `DATE` (chrono `NaiveDate`). Booleans are
`BOOLEAN`. Foreign keys are enforced (`PRAGMA foreign_keys=ON`).

```sql
entity_types (
  id TEXT PK, name TEXT NOT NULL, description TEXT, icon TEXT,
  is_location BOOLEAN NOT NULL DEFAULT 0,
  default_template_id TEXT NULL REFERENCES entity_templates(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
)

entities (
  id TEXT PK, name TEXT NOT NULL, description TEXT,
  entity_type_id TEXT NOT NULL REFERENCES entity_types(id) ON DELETE RESTRICT,
  parent_id TEXT NULL REFERENCES entities(id) ON DELETE RESTRICT,
  archived BOOLEAN NOT NULL DEFAULT 0,
  asset_id BIGINT NOT NULL DEFAULT 0,
  import_ref TEXT, notes TEXT,
  quantity DOUBLE NOT NULL DEFAULT 1,
  insured BOOLEAN NOT NULL DEFAULT 0,
  serial_number TEXT, model_number TEXT, manufacturer TEXT,
  lifetime_warranty BOOLEAN NOT NULL DEFAULT 0, warranty_expires DATE, warranty_details TEXT,
  purchase_date DATE, purchase_from TEXT, purchase_price_cents BIGINT NOT NULL DEFAULT 0,
  sold_date DATE, sold_to TEXT, sold_price_cents BIGINT NOT NULL DEFAULT 0, sold_notes TEXT,
  sync_child_entity_locations BOOLEAN NOT NULL DEFAULT 0,
  created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
)
-- indexes: name, parent_id, entity_type_id, archived, asset_id

tags (
  id TEXT PK, name TEXT NOT NULL, description TEXT, color TEXT, icon TEXT,
  parent_id TEXT NULL REFERENCES tags(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
)

tag_entities (
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  PRIMARY KEY (tag_id, entity_id)
)

attachments (
  id TEXT PK,
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,           -- photo | manual | warranty | attachment | receipt
  is_primary BOOLEAN NOT NULL DEFAULT 0,
  title TEXT NOT NULL DEFAULT '',
  mime_type TEXT NOT NULL,
  sha256 TEXT NOT NULL,         -- file key under $DATA_DIR/originals/
  size_bytes BIGINT NOT NULL,
  created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
)
-- Homebox "thumbnail" attachment rows are NOT stored here; see thumbnails.

thumbnails (
  attachment_id TEXT NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
  size INTEGER NOT NULL,        -- 300 | 500 | 1200
  mime_type TEXT NOT NULL,      -- image/webp
  width INTEGER NOT NULL, height INTEGER NOT NULL,
  data BLOB NOT NULL,
  created_at TIMESTAMP NOT NULL,
  PRIMARY KEY (attachment_id, size)
)

entity_fields (
  id TEXT PK,
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  name TEXT NOT NULL, description TEXT,
  kind TEXT NOT NULL,           -- text | number | boolean | time
  text_value TEXT, number_value BIGINT, boolean_value BOOLEAN NOT NULL DEFAULT 0, time_value TIMESTAMP,
  created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
)

entity_templates (
  id TEXT PK, name TEXT NOT NULL, description TEXT, notes TEXT,
  default_quantity DOUBLE NOT NULL DEFAULT 1, default_insured BOOLEAN NOT NULL DEFAULT 0,
  default_name TEXT, default_description TEXT, default_manufacturer TEXT, default_model_number TEXT,
  default_lifetime_warranty BOOLEAN NOT NULL DEFAULT 0, default_warranty_details TEXT,
  include_warranty_fields BOOLEAN NOT NULL DEFAULT 0, include_purchase_fields BOOLEAN NOT NULL DEFAULT 0,
  include_sold_fields BOOLEAN NOT NULL DEFAULT 0,
  default_tag_ids TEXT,         -- JSON array, stored verbatim
  location_id TEXT NULL REFERENCES entities(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
)

template_fields (
  id TEXT PK,
  template_id TEXT NOT NULL REFERENCES entity_templates(id) ON DELETE CASCADE,
  name TEXT NOT NULL, description TEXT, kind TEXT NOT NULL,
  text_value TEXT, number_value BIGINT, boolean_value BOOLEAN NOT NULL DEFAULT 0, time_value TIMESTAMP,
  created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
)

settings (key TEXT PK, value TEXT NOT NULL)
-- seeded: ('currency', 'USD')
```

Notes:

- `kind` is used instead of `type` to avoid a Rust keyword. Homebox's `primary`
  becomes `is_primary` for the same reason in SQL, and is exposed as `primary`
  in GraphQL.
- Rust models use newtypes where the value has rules: `AssetId(i64)` with
  `Display` giving `000-007`, `Cents(i64)`. Enum columns (`kind`) map to Rust
  enums with `FromSql`/`ToSql` on `Text` so an invalid string can't be inserted.
- No `home`/`group` table. The database *is* the home. When auth arrives it adds
  `users(id, name, role)` and a session table, nothing on the inventory side.
- Built-in types are seeded by a migration when the table is empty: `Location`
  (`is_location`), `Item`. The importer maps Homebox's `global.location` and
  `global.item` names onto these seeded rows by *name*, keeping the seeded ids,
  so a fresh install and an imported install have the same built-in ids. All
  other types are imported with their Homebox ids.

## 6. GraphQL schema

Juniper converts `snake_case` to `camelCase`. Booleans do not use an `is`
prefix except where the word is the noun itself (`isLocation` stays, matching
Homebox's API vocabulary). Money is exposed as `Int` cents; a home's total value
therefore caps at about 21 million in the display currency, which is accepted.

```graphql
type Summary {
  totalValueCents: Int!
  currency: String!
  totalItems: Int!
  totalLocations: Int!
  totalTags: Int!
}

type EntityType {
  id: ID!  name: String!  description: String  icon: String
  isLocation: Boolean!
  entityCount: Int!
  createdAt: DateTime!  updatedAt: DateTime!
}

type Entity {
  id: ID!  name: String!  description: String
  entityType: EntityType!
  isLocation: Boolean!            # convenience: entityType.isLocation
  parent: Entity
  parentId: ID                    # parent's id, without loading the parent entity
  ancestors: [Entity!]!           # root first, for breadcrumbs
  childLocations: [Entity!]!      # direct children whose type is a location
  items: [Entity!]!               # direct children whose type is not a location
  archived: Boolean!
  assetId: String                 # "000-007" or null when 0
  quantity: Float!  insured: Boolean!
  serialNumber: String  modelNumber: String  manufacturer: String  notes: String
  lifetimeWarranty: Boolean!  warrantyExpires: LocalDate  warrantyDetails: String
  purchaseDate: LocalDate  purchaseFrom: String  purchasePriceCents: Int!
  soldDate: LocalDate  soldTo: String  soldPriceCents: Int!  soldNotes: String
  tags: [Tag!]!
  attachments: [Attachment!]!
  primaryPhoto: Attachment
  fields: [EntityField!]!
  createdAt: DateTime!  updatedAt: DateTime!
}

type Tag { id: ID!  name: String!  description: String  color: String  icon: String  parent: Tag  entityCount: Int! }

type Attachment {
  id: ID!  kind: AttachmentKind!  primary: Boolean!  title: String!  mimeType: String!  sizeBytes: Int!
  url: String!                    # /attachments/{id}
  thumbnailUrl(size: Int! = 500): String   # /attachments/{id}/thumb/{size}; null for non-images
}
enum AttachmentKind { PHOTO MANUAL WARRANTY ATTACHMENT RECEIPT }

type EntityField { id: ID!  name: String!  kind: FieldKind!  textValue: String  numberValue: Int  booleanValue: Boolean!  timeValue: DateTime }
enum FieldKind { TEXT NUMBER BOOLEAN TIME }

type Query {
  summary: Summary!
  entityTypes: [EntityType!]!
  locations: [Entity!]!                   # every location entity, flat, sorted by name
  entity(id: ID!): Entity
  rootItems: [Entity!]!                   # non-location entities with no parent (homeless items)
  tags: [Tag!]!
  search(query: String!, limit: Int = 25): [Entity!]!   # case-insensitive substring on name
}

input EntityInput {
  name: String!  description: String  entityTypeId: ID!  parentId: ID
  archived: Boolean  quantity: Float  insured: Boolean
  serialNumber: String  modelNumber: String  manufacturer: String  notes: String
  lifetimeWarranty: Boolean  warrantyExpires: LocalDate  warrantyDetails: String
  purchaseDate: LocalDate  purchaseFrom: String  purchasePriceCents: Int
  soldDate: LocalDate  soldTo: String  soldPriceCents: Int  soldNotes: String
  tagIds: [ID!]
}
input EntityTypeInput { name: String!  description: String  icon: String  isLocation: Boolean! }
input TagInput { name: String!  description: String  color: String  icon: String  parentId: ID }

type Mutation {
  createEntity(input: EntityInput!): Entity!
  updateEntity(id: ID!, input: EntityInput!): Entity!
  deleteEntity(id: ID!): Boolean!          # error if it has children
  createEntityType(input: EntityTypeInput!): EntityType!
  updateEntityType(id: ID!, input: EntityTypeInput!): EntityType!
  deleteEntityType(id: ID!): Boolean!      # error if entities use it
  createTag(input: TagInput!): Tag!
  updateTag(id: ID!, input: TagInput!): Tag!
  deleteTag(id: ID!): Boolean!
  deleteAttachment(id: ID!): Boolean!
  setPrimaryPhoto(attachmentId: ID!): Entity!
}
```

`DateTime` and `LocalDate` are juniper's chrono scalars. Phase 1 ships only `Summary`
and `Query.summary` (dummy data); later phases add the rest.

Validation rules enforced in `svc`, tested at the GraphQL seam:

- `parentId` must not be the entity itself or any of its descendants (cycle).
- A parent must be a location-type entity **unless** the child is not a location
  either (items may nest in items). A location may not be placed under an item.
- Changing a type from location to non-location is refused while any entity of
  that type has children that are locations.
- `assetId` is assigned on create as `max(asset_id)+1` when the input has none.

## 7. HTTP endpoints

| Method | Path | Notes |
|--------|------|-------|
| POST | `/graphql` | juniper_axum. GraphiQL served at `/graphiql` in debug builds only. |
| GET | `/attachments/{id}` | Streams the original from `$DATA_DIR/originals/<sha256>` with its stored MIME type and `Content-Disposition: inline; filename="<title>"`. Immutable cache headers. |
| GET | `/attachments/{id}/thumb/{size}` | `size` is rounded **up** to the smallest allowed size ≥ it; above 1200 clamps to 1200; a non-positive or non-numeric size is HTTP 400. Non-image attachments 404. Immutable cache headers. |
| POST | `/api/upload/{entityId}` | Multipart photo upload (phase 5). Field `file`, optional `primary=true`. Returns the `Attachment` as JSON. |
| GET | `/*` | Embedded SPA. Unknown paths fall back to `index.html`. |

Both `url` and `thumbnailUrl` carry `?v=<sha256 prefix>`, the first 12 hex
characters of the attachment's sha256, so the SPA can cache a URL forever and
still see new bytes after a re-import. The ETag is `"<sha256>"` on the
original and `"<sha256>-<size>"` on a thumbnail.

A healthcheck needs no route: the `healthcheck` subcommand connects to
`127.0.0.1:$PORT`, sends `GET / HTTP/1.0`, reads one byte, exits 0/1.

## 8. Homebox import

`home-tracker import <path>` accepts a `.zip` or an already-exploded directory.
It reads `manifest.json` first and refuses anything but `schemaVersion: 1` with
a clear error naming the version it found.

Reading order and mapping (Homebox column → ours):

1. `entity_types.json`: `global.location`/`global.item` map by name onto the
   seeded built-ins; others upsert by id. `entity_type_default_template` is
   applied in a second pass after templates exist.
2. `entity_templates.json`, `template_fields.json`.
3. `tags.json`: upsert, then a second pass sets `tag_children` → `parent_id`.
4. `entities.json`: upsert all with `parent_id = NULL`, then a second pass sets
   `entity_children` → `parent_id` (the column holds the **parent** id despite
   its name). `entity_type_entities` → `entity_type_id`. `purchase_price` and
   `sold_price` floats → cents by `(price * 100).round()`. Datetime strings on
   `purchase_date`/`sold_date`/`warranty_expires` → the date part.
5. `entity_fields.json`.
6. `tag_entities.json`: upsert pairs.
7. `attachments.json`: rows with `type = "thumbnail"` are skipped as attachments.
   For every other row: copy `attachments/<id>` into `$DATA_DIR/originals/` under
   its SHA-256, computed while copying. Homebox's `path` column is an opaque
   storage key and is ignored. Record `size_bytes`, upsert the row. If the row's `attachment_thumbnail` points at a
   thumbnail row whose blob exists, store that blob as the `500` variant with the
   decoded dimensions.
8. `maintenance_entries.json`, `notifiers.json`: counted and warned about, not read.

Everything runs in one transaction per table group so a failure leaves the
database at the previous consistent state. The command prints a summary table of
rows inserted/updated per table and warnings. Exit code is non-zero on any error.

The importer is pure with respect to I/O boundaries: `import::tables` holds the
serde structs, `import::run` takes a `Source` trait object (`ZipSource` /
`DirSource`) that yields table JSON and attachment bytes, so tests build a
`DirSource` from a synthetic fixture under `tests/fixtures/homebox-mini/`
(three types, four entities in a two-level tree, two tags, one photo with a
thumbnail, one custom field).

## 9. Attachments and thumbnails

- Originals: `$DATA_DIR/originals/<sha256>`. Content addressing dedupes byte-
  identical uploads and mirrors Homebox's layout. Deleting an attachment removes
  the file only when no other attachment row shares the hash.
- Thumbnails: table `thumbnails`, keyed on `(attachment_id, size)`. Allowed sizes
  are `const THUMB_SIZES: [u32; 3] = [300, 500, 1200]`. Request size is rounded
  up to the first allowed size ≥ it, and anything larger than 1200 clamps to
  1200. A cache hit streams the blob. A miss:
  1. checks the attachment's MIME type is one of `image/jpeg`, `image/png`,
     `image/gif`, `image/webp` (404 otherwise; AVIF is deferred, it needs `nasm`);
  2. takes a per-key permit from `ThumbnailService { in_flight: Mutex<HashMap<(Uuid,u32), Arc<Notify>>> }`
     so concurrent misses for the same key wait for the first generator;
  3. in `spawn_blocking`: decode with `image`, apply EXIF orientation, resize to
     fit `size×size` preserving aspect ratio (never upscale), encode WebP quality
     80 with the `webp` crate;
  4. inserts the row and streams it.
- The same `generate(attachment_id, size)` function is what a future
  pregeneration job will call.
- Uploads (phase 5): multipart to `/api/upload/{entityId}`, streamed to a temp
  file while hashing, moved into `originals/`, row inserted, first photo on an
  entity becomes primary automatically.

## 10. Authentication readiness

- `GraphQLContext.actor: Actor` where `enum Actor { Anonymous, User { id, role } }`
  and `enum Role { ReadOnly, Write }`. In v1 the context is built with
  `Actor::Anonymous` and `Actor::can_write()` returns `true` for it. Every
  mutation resolver starts with `ctx.require_write()?`. When auth lands, only
  the context constructor and that one method change.
- No data is scoped by user or group. Auth adds `users` and `sessions` tables
  and a middleware that fills `actor`; the inventory schema is untouched.
- The upload and attachment handlers read the same `Actor` from request
  extensions so the file paths get the same gate.

## 11. Frontend

**Routes** (react-router 7, lazy pages inside a `PageTemplate` layout):

| Path | Page |
|------|------|
| `/` | HomePage: four stat cards, then root items (phase 3) |
| `/locations/:id` | LocationPage: breadcrumb, child location cards, item list, "add item" |
| `/items/:id` | ItemPage: photo, details, tags, custom fields, edit form |
| `/types` | EntityTypesPage: list, create/edit/delete |
| `/tags` | TagsPage |
| `/search?q=` | SearchPage |

An `/items/:id` route whose entity is a location redirects to `/locations/:id`
and vice versa, so links can be built from either.

**Layout.** `PageTemplate` renders a header (brand, search box, theme toggle), a
left sidebar containing `LocationTree` (expand/collapse, current node
highlighted, persisted expanded set in `localStorage`), and an `<Outlet/>`.
Below the `md` breakpoint the sidebar becomes a drawer behind a hamburger.

**Theming.** `site/src/theme/tokens.css` declares semantic variables per theme:

```css
:root, [data-theme="dark"] { --t-bg: …; --t-surface: …; --t-surface-raised: …; --t-border: …;
  --t-text: …; --t-text-muted: …; --t-accent: …; --t-accent-text: …; --t-danger: …; --t-success: …; }
[data-theme="light"] { /* stub values */ }
@theme inline { --color-bg: var(--t-bg); --color-surface: var(--t-surface); … }
```

Components use only the semantic classes (`bg-surface`, `text-muted`,
`border-border`, `bg-accent`), never raw palette classes like `bg-zinc-900`,
so a theme is a block of variables. `ThemeProvider` sets `data-theme` on
`<html>` from `localStorage` (default `dark`) and `useTheme()` exposes
`{theme, setTheme}`. `index.css` restores `cursor: pointer` on buttons exactly as
chore-tracker does.

**Hooks.** One hook per operation, named after it: `useSummary`, `useLocations`,
`useEntity`, `useEntityTypes`, `useTags`, `useSearch`, `useCreateEntity`,
`useUpdateEntity`, `useDeleteEntity`, `useUploadPhoto` (fetch, not Apollo).
Mutations refetch the queries they invalidate (`locations`, `entity`,
`summary`). Every hook has a vitest test using Apollo's `MockedProvider`.

**Thumbnails in the UI.** A `<Thumb attachment size>` component builds the URL
with the requested size (300 in lists and the tree, 500 on cards, 1200 on the
item page) and the server does the rounding.

## 12. Configuration, build, operations

| Env var | Default | Meaning |
|---------|---------|---------|
| `PORT` | `7008` | listen port |
| `LISTEN_ADDRESS` | `::` | address only, dual-stack as in chore-tracker |
| `DATABASE_URL` | `data/db.sqlite` | SQLite path; parent dir is created |
| `DATA_DIR` | `data` | originals live in `$DATA_DIR/originals` |
| `RUST_LOG` | `info` | tracing filter |

Docker: `env.prod` sets `DATABASE_URL=/data/db.sqlite` and `DATA_DIR=/data`,
`VOLUME /data`, `EXPOSE 7008`, `HEALTHCHECK CMD ["/home-tracker","healthcheck"]`.
Build is the chore-tracker three-stage musl pattern (dependency cache stub,
real build, `FROM scratch`). `libsqlite3-sys` is `bundled`. `webp` links
libwebp via `cc`, which the musl image provides.

CI (`.github/workflows/rust.yml`): `coverage` (tarpaulin, needs an empty
`site/build/index.html` first), `build-site` (node 22, yarn, uploads
`site/build`), `build-docker` (needs `build-site`, Tailscale, metadata-action,
buildx with `network=host`, push, Watchtower). Image name `home-tracker`.
Secrets/vars are the same names as chore-tracker.

Local dev: `cargo run` (backend on 7008), `cd site && yarn dev` (Vite proxies
`/graphql`, `/attachments`, `/api` to 7008). `justfile`: `test` (watchexec),
`cover` (llvm-cov), `import path` (runs the CLI against `homebox-backup/`).
Husky pre-commit runs `lint-staged` and `CI=true yarn test` in `site/`. Rust
pre-commit is `cargo fmt --check` and `cargo clippy -- -D warnings` via the same
hook.

## 13. Testing

- **Value types** (`money`, `asset_id`, thumbnail size rounding): unit tests.
- **Services**: tests against a fresh temp-file SQLite with migrations applied
  (`db::test_pool()`), one module per service. Stats have a characterization
  test whose fixture mirrors the Homebox formula cases: archived item, sold
  item, quantity > 1, a location with a price.
- **GraphQL seam**: `axum-test` integration tests in `tests/` posting real
  queries, asserting the JSON shape. Every query and mutation has at least one.
- **Importer**: end-to-end against `tests/fixtures/homebox-mini/`; asserts row
  counts, parent links, cents conversion, thumbnail stored at 500, idempotence
  (run twice, same counts, updated `updated_at` only where data changed), and
  the schema-version refusal.
- **Thumbnails**: generate from a fixture JPEG, assert dimensions ≤ box, WebP
  magic bytes, cache hit on second call, and rounding of sizes.
- **Frontend**: vitest + testing-library. Hook tests with `MockedProvider`;
  component tests for `LocationTree` (expand/collapse, highlight), `StatCard`,
  `EntityForm` validation, `Thumb` URL building; a smoke test that `App`
  renders the home page.
- **Coverage** reported by tarpaulin in CI as in chore-tracker.

## 14. Invariants this design depends on

Listed so a later change touching one of these funnels can find who relies on it.

- The importer keeps Homebox UUIDs as primary keys. Anything that generates ids
  must use v7 and must not assume ids are time-ordered for imported rows.
- Built-in types are recognised on import by their Homebox names
  (`global.location`, `global.item`). The importer maps Homebox's
  `global.location`/`global.item` onto the seeded ids by their fixed ids, so
  deleting a seeded row breaks re-import; renaming it does not.
- `thumbnails` rows are derived data. Any code path that replaces an
  attachment's bytes must delete its thumbnail rows.
- `entities.parent_id` has `ON DELETE RESTRICT`; the "refuse delete with
  children" rule is enforced by the database as well as the service.
- Total Value counts sold items. A future "exclude sold" option must be a
  separate query, not a change to `summary`.
- Semantic theme tokens are the only colour source for components; a component
  using a raw palette class silently breaks the second theme.

## 15. Phases

Each phase: plan → subagent-driven development → final review → docs → merge.
A phase's plan may split into more tasks than listed; the exit criteria are the
contract.

**Phase 1: walking skeleton.**
Backend: crate, clap (`serve`/`import` stub that errors "not yet"/`healthcheck`),
tracing, config, dual-stack bind, r2d2 pool with PRAGMAs, embedded migrations
(`settings` table only, seeded currency), juniper `Query.summary` returning
fixed dummy numbers, axum router with compression and embedded SPA, jemalloc,
Dockerfile, `.dockerignore`, `env.prod`, `justfile`, CI workflow.
Frontend: Vite/React/TS/Apollo/Tailwind v4 scaffold, theme tokens and
`ThemeProvider`, `PageTemplate` with header + empty sidebar, `HomePage` with
four `StatCard`s fed by `useSummary`, vitest, eslint/prettier, husky.
Exit: `cargo test`, `cargo clippy -D warnings`, `yarn test`, `yarn build` pass;
`cargo run` serves the built site and the home page shows the dummy stats;
`docker build` succeeds locally.

**Phase 2: real data and import.**
Full schema migration, models, `svc` for reads, the whole `Query` side of §6,
real `summary`, value types, importer with fixture, `just import`. Exit: Steve's
backup imports; `summary` matches Homebox's numbers for it (Total Value
computed by hand from the JSON as the check); all `Query` fields have
integration tests.

**Phase 3: browsing.**
`LocationTree` sidebar, `LocationPage`, `ItemPage` (read-only), breadcrumbs,
search, `Thumb` component backed by the thumbnail endpoint and original
serving (read side of §9, including on-demand generation, since browsing
imported photos needs it). Exit: every imported entity reachable by clicking.

**Phase 4: editing.**
All mutations in §6, `EntityForm` (create on the location page, edit on the item
page, parent picker, tag picker), types page, tags page, delete with the
children rule. Exit: an item can be created, moved, tagged, edited, deleted;
stats update.

**Phase 5: photos.**
Upload endpoint, `useUploadPhoto`, photo gallery on the item page, primary
photo selection, attachment delete with hash-refcount file removal. Exit: a
photo uploaded from the browser appears at all three thumbnail sizes.

## 16. Phase-boundary protocol

At each boundary the driver re-reads this spec and the previous phase's review
findings, writes the next plan, and if a question arises posts it in chat with
the default it will take, waits ten minutes, then proceeds on the default.
Merging to `main` and pushing happens at the end of each phase after the final
review and docs are committed.
