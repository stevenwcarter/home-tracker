# home-tracker

A self-hosted home inventory app that replaces [Homebox](https://homebox.software/)
for a single household. One instance tracks one home: locations, items nested
inside them, tags, and photos. There is no authentication yet; the server
trusts every request as a writer. Phase 1 shipped the backend, the database,
and a home page with four statistic cards. Phase 2 adds the full inventory
schema, a read-only GraphQL query surface over it, and an importer that reads
a Homebox backup so the app can run on real data.

## Run with Docker

```bash
docker run -p 7008:7008 -v ht-data:/data home-tracker:dev
```

The image is `scratch`-based (musl, no shell) and exposes a `HEALTHCHECK` via
the binary's own `healthcheck` subcommand. `ht-data` holds the SQLite database
and, from a later phase, uploaded photos. `just docker-build` builds this
`home-tracker:dev` tag locally; CI publishes tagged images to the registry
on every push.

| Env var | Default | Meaning |
|---------|---------|---------|
| `PORT` | `7008` | Listen port |
| `LISTEN_ADDRESS` | `::` | Bind address only (dual-stack by default) |
| `DATABASE_URL` | `data/db.sqlite` | SQLite path; parent directory is created |
| `DATA_DIR` | `data` | Originals live under `$DATA_DIR/originals` |
| `RUST_LOG` | `info` | tracing filter |

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
300/500/1200px and cached in the `thumbnails` table thereafter). Both URLs
carry `?v=<sha256 prefix>`, so they can be cached by the browser forever and
still pick up new bytes after a re-import; each response also sets a matching
`ETag`.

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
home if it had none. On an item page, each attachment also has its own
Delete and, for a photo that is not already primary, Make primary.

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
│   ├── api/            axum handlers: graphql
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
