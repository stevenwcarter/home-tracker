# home-tracker

A self-hosted home inventory app that replaces [Homebox](https://homebox.software/)
for a single household. One instance tracks one home: locations, items nested
inside them, tags, and photos. There is no authentication yet; the server
trusts every request as a writer. Phase 1 shipped the backend, the database,
and a home page with four statistic cards. Phase 2 added the full inventory
schema, a GraphQL query surface over it, and an importer that reads a Homebox
backup so the app can run on real data. Phases 3 and 4 added browsing and
editing, phase 5 added photo uploads with a gallery on every entity page,
and phase 6 adds a settings screen where an OpenAI-compatible model is
configured for the AI item ingest that follows.

## Run with Docker

```bash
docker run -p 7008:7008 -v ht-data:/data home-tracker:dev
```

The image is `scratch`-based (musl, no shell) and exposes a `HEALTHCHECK` via
the binary's own `healthcheck` subcommand. `ht-data` holds the SQLite database
and the photo originals, imported or uploaded. `just docker-build` builds this
`home-tracker:dev` tag locally; CI publishes tagged images to the registry
on every push.

| Env var | Default | Meaning |
|---------|---------|---------|
| `PORT` | `7008` | Listen port |
| `LISTEN_ADDRESS` | `::` | Bind address only (dual-stack by default) |
| `DATABASE_URL` | `data/db.sqlite` | SQLite path; parent directory is created |
| `DATA_DIR` | `data` | Originals live under `$DATA_DIR/originals` |
| `RUST_LOG` | `info` | tracing filter |
| `OPENAI_API_KEY` | unset | AI API key; overrides the one saved in settings |
| `OPENAI_BASE_URL` | unset | OpenAI-compatible endpoint; overrides the saved base URL |

An API key saved from the settings screen is stored in plaintext in the
SQLite database, so treat the database file as a secret (see Settings).

The Docker image ships `env.prod`, which sets `DATABASE_URL=/data/db.sqlite`
and `DATA_DIR=/data` to match the `/data` volume.

## Local development

```bash
just site-placeholder     # once: creates a stub site/build/index.html so cargo build has something to embed
cargo run                 # backend on :7008
cd site && yarn dev       # frontend dev server, proxies /graphql etc. to :7008
just test                 # cargo test, re-run on file changes
cd site && yarn test      # vitest
```

Run `yarn install` once at the repo root too; it wires up the husky
pre-commit hook via the root `prepare` script.

## Importing a Homebox backup

```bash
home-tracker import backup.zip
home-tracker import homebox-backup   # an exploded backup directory works too
```

Reads a Homebox export, either the zip Homebox produces or that zip already
unzipped into a folder, and upserts it into the database: entity types,
entity templates and their template fields, locations and items, tags,
custom fields, and attachments with their thumbnails. Every row is upserted
by its Homebox UUID, so re-running the same backup is safe: it refreshes
existing rows in place and never deletes anything. Homebox's two built-in
entity types (`global.location`, `global.item`) are mapped by name onto the
seeded location and item types instead of being inserted again. Homebox's
attachment `path` column is ignored: each file is found in the backup by its
attachment id, content-addressed by SHA-256, and written to
`$DATA_DIR/originals/<sha256>`. Thumbnails are stored in the database at
Homebox's original 500px size. Maintenance entries and notifiers are not
imported; the command prints one warning per category with the count. The
command's output is a per-table count of rows inserted, updated, and
skipped, plus any warnings.

## Browsing

The site opens on a home page with stat cards and root locations, then lets you
drill in from there: a left sidebar tree (built client-side from the flat
`locations` list, expand/collapse persisted in `localStorage`) links to a
location page (breadcrumb, child locations, items with thumbnails) and an item
page (large photo, details, tags). A header search box queries across items
and locations by name.

Every attachment is reachable at `GET /attachments/{id}` (the original) and
`GET /attachments/{id}/thumb/{size}` (a thumbnail, generated on demand at
300/500/1200px and cached in the `thumbnails` table thereafter, keyed by
the original's SHA-256, so attachments with the same bytes share them). Both URLs
carry `?v=<sha256 prefix>`, so they can be cached by the browser forever and
still pick up new bytes after a re-import; each response also sets a matching
`ETag`.

## Uploading photos

Every item and location page has a Photos section: an Add photos button (it
takes several files at once) and a drop zone over the gallery. Files upload
one at a time, each with its own status line; a failed file shows why and the
rest carry on. The gallery shows 300px thumbnails with a Primary badge, a Make
primary button and a Delete button (confirmed in a dialog); clicking a
thumbnail opens a 1200px viewer with Previous/Next, closed with Escape.

- **Formats:** JPEG, PNG, GIF and WebP. The server decides the format from
  the file's first bytes, never from its name or the browser's content type,
  so a renamed text file is refused (415). A file in one of those formats
  that does not decode (a damaged header or a truncated body), or whose image
  is over 8192px on a side or would need more than 256 MiB to decode, is
  refused too (422), so every stored photo can be thumbnailed.
- **Size:** at most 25 MiB per file (413 above that); the request may carry
  64 KiB more for the form around it. Only the upload route has this limit.
  The site refuses a larger file before sending it, since a browser cut off
  mid-upload usually reports a network error instead of the 413.
- **Primary photo:** an entity's first photo becomes its primary photo. A
  later upload sent with `primary=true` takes over, so exactly one photo is
  primary. Deleting the primary photo promotes the earliest remaining photo
  that can be thumbnailed (an imported HEIC is passed over).
- **Dedupe:** originals are stored once per content hash under
  `$DATA_DIR/originals/<sha256>`. Uploading the same bytes to a second entity
  adds a row, not a file; the file is removed when its last row is deleted.
- **Thumbnails:** the 300px one is made during the upload itself; 500 and
  1200px are generated on first request (all WebP, never upscaled) and
  cached in the database. An original that fails to
  decode is remembered for the life of the process, so it is not decoded
  again on every request; its thumbnails answer 404.

The endpoint is `POST /api/upload/{entityId}` with a multipart `file` field
and an optional `primary` field (`true`, `1`, `on` or `yes`). It answers 201
with the stored attachment as JSON (`id`, `kind`, `primary`, `title`,
`mimeType`, `sizeBytes`, `url`, `thumbnailUrl`), or an error status with
`{ "error": "..." }`: 400 for a missing, repeated or malformed `file` field,
403 when the caller may not write, 404 for an unknown entity, and 413, 415
or 422 as above. The title is the file's name without any path, control
characters or invisible format characters, at most 255 characters,
defaulting to `photo.<ext>`.

```bash
curl -F file=@shelf.jpg -F primary=true http://localhost:7008/api/upload/<entity id>
```

## Editing

A location page has Add item, Add location, Edit and Delete buttons; an item
page has Edit and Delete. "Add" opens `/locations/:id/new` with the type
preselected (`?type=` picks Location over the default Item). `/new` creates
a parentless entity the same way; the home page's Add location and Add item
links open it. Edit
opens `/locations/:id/edit` or `/items/:id/edit`. All these routes render the
same `EntityForm`, split into Basics, Purchase, Warranty, Sold and Details
sections; Purchase, Warranty and Sold start collapsed when the selected type
is a location. Saving a create or edit goes to the entity's own page.

Delete never uses the browser's `confirm()`; it opens an in-app confirmation
dialog. A location's Delete is disabled while it still holds anything. Deleting an item or location navigates to its parent location, or
home if it had none. Photos are managed in the gallery (see Uploading
photos); an item's other attachments are listed under Attachments, each with
its own Delete.

`/types` and `/tags` list every entity type and tag with how many entities
use it, and let you create, edit in place, and delete from the same table.

**What validation refuses:**
- A blank or whitespace-only name, for an entity, type, or tag.
- An unknown entity type, parent id, or tag id.
- Placing a location under an item (items may nest under items, but a
  location may not).
- Moving an entity under itself or one of its own descendants, or
  reparenting a tag under itself or one of its own descendants (a cycle).
- A negative quantity or a negative purchase/sold price.
- Deleting an entity that still contains other entities; the error names how
  many.
- Deleting an entity type that is one of the built-in Location/Item types, or
  still used by any entity; the error names the type or the count.
- Turning a location type into a non-location type while any entity of that
  type still holds a location child, or turning a type into a location type
  while any entity of that type sits inside an item of another type.
- A tag colour that is not a hex value (`#rgb`, `#rrggbb` or `#rrggbbaa`).

## Settings

The Settings link in the header opens `/settings`, a tabbed screen that
lands on its only tab so far, AI (`/settings/ai`). The AI tab configures
the OpenAI-compatible model the server calls; the browser never talks to
the model or sees the key.

- **Base URL:** the endpoint, default `https://api.openai.com/v1`; any
  OpenAI-compatible server works. Calls go to `{base URL}/chat/completions`.
  Trailing slashes are stripped; the URL must be `http://` or `https://`
  with a host, and one carrying a `user:password@` part is refused.
- **Vision model:** describes each photo. Default `gpt-5-mini`.
- **Synthesis model:** combines the photo descriptions into one item
  suggestion, and answers the connection test. Default `gpt-5-mini`.
- **Extra instructions:** optional free text, at most 4000 characters,
  appended to both system prompts.
- **API key:** write-only. The field is always blank: once a key is saved
  it reads "Key saved" with a Clear key button, and no query, error message
  or log line ever contains the key again. Leaving the field blank keeps
  the saved key; typing a new one replaces it; Clear key removes it on Save.

**Test connection** sends one tiny request (the synthesis model is asked to
reply "OK", capped at 8 output tokens) and shows which model answered and
how many milliseconds it took, or the provider's error. It is disabled until
a key is set, and it costs a few tokens.

**Environment variables win.** When `OPENAI_API_KEY` or `OPENAI_BASE_URL`
is set, it overrides the saved value; the tab shows that field disabled with
"Set from OPENAI_... in the environment", a save that would change it is
refused with a message naming the variable, and the other fields still save.
An environment key is never written to the database.

**The SQLite file holds the API key: treat it as a secret.** A key saved
here is stored in plaintext in the `settings` table, so the database file
and every backup of it carry the key.

Each model call has a 60 s timeout (10 s to connect) and is retried on 429
and 5xx after 1 s and then 4 s. A provider that refuses
`response_format: json_schema` is asked once more with `json_object`, and
one that refuses `max_completion_tokens` once more with `max_tokens`. Each
call logs its model, status, latency and token counts at `info`; an error
body is logged at `warn`, cut to 500 characters, with the key masked.

## Project layout

```
home-tracker/
├── Cargo.toml, rustfmt.toml, diesel.toml
├── migrations/         diesel migrations, embedded at build time
├── src/
│   ├── main.rs         clap: serve (default), import, healthcheck
│   ├── lib.rs          tracing setup, module re-exports
│   ├── config.rs       env to Config
│   ├── net.rs          dual-stack bind
│   ├── db.rs           r2d2 pool, PRAGMAs, embedded migrations, TestDb
│   ├── schema.rs       diesel print-schema output (generated)
│   ├── asset_id.rs     AssetId, Homebox-style "000-007" asset numbers
│   ├── money.rs        Cents, integer-cents money
│   ├── kinds.rs        closed string enums stored as lowercase TEXT
│   ├── models/         Diesel Queryable/Selectable structs, one file per table
│   ├── svc/            business logic, one file per aggregate
│   ├── graphql/        juniper: context, schema, query, objects/ (one file per type)
│   ├── import/         Homebox backup importer: source, tables, run, report
│   ├── ai/             OpenAI-compatible client, fake, prompts, OPENAI_* overrides
│   ├── api/            axum handlers: graphql, attachments, upload, actor middleware
│   ├── routes.rs       router, compression, embedded SPA, /assets cache
│   └── healthcheck.rs
├── site/               Vite + React 19 + TypeScript + Apollo + Tailwind v4
│   └── src/
│       ├── theme/          tokens.css, ThemeProvider, useTheme
│       ├── hooks/          one hook per GraphQL operation
│       ├── components/
│       ├── page/
│       ├── types/
│       └── utils/
├── tests/              Rust integration tests
├── Dockerfile, .dockerignore, env.prod, justfile, .husky/
└── .github/workflows/rust.yml
```

## Docs

- Design spec: [`docs/superpowers/specs/2026-09-25-home-tracker-design.md`](docs/superpowers/specs/2026-09-25-home-tracker-design.md)
- Phase 1 plan: [`docs/superpowers/plans/2026-09-25-phase-1-walking-skeleton.md`](docs/superpowers/plans/2026-09-25-phase-1-walking-skeleton.md)
- Phase 2 plan: [`docs/superpowers/plans/2026-09-26-phase-2-real-data-and-import.md`](docs/superpowers/plans/2026-09-26-phase-2-real-data-and-import.md)
- Phase 3 plan: [`docs/superpowers/plans/2026-09-26-phase-3-browsing.md`](docs/superpowers/plans/2026-09-26-phase-3-browsing.md)
- Phase 4 plan: [`docs/superpowers/plans/2026-09-27-phase-4-editing.md`](docs/superpowers/plans/2026-09-27-phase-4-editing.md)
- Phase 5 plan: [`docs/superpowers/plans/2026-09-28-phase-5-photos.md`](docs/superpowers/plans/2026-09-28-phase-5-photos.md)
- AI ingest spec: [`docs/superpowers/specs/2026-09-26-ai-ingest-design.md`](docs/superpowers/specs/2026-09-26-ai-ingest-design.md)
- Phase 6 plan: [`docs/superpowers/plans/2026-09-26-phase-6-settings-and-ai-client.md`](docs/superpowers/plans/2026-09-26-phase-6-settings-and-ai-client.md)
