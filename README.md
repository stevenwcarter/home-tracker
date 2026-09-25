# home-tracker

A self-hosted home inventory app that replaces [Homebox](https://homebox.software/)
for a single household. One instance tracks one home: locations, items nested
inside them, tags, and photos. There is no authentication yet; the server
trusts every request as a writer. Phase 1 (this walking skeleton) ships the
backend, the database, and a home page with four live statistic cards backed
by fixed dummy numbers.

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
```

**Phase 2, not yet available.** In phase 1 this subcommand exits with an
error explaining it is not implemented yet.

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
│   ├── svc/            business logic (settings, stats)
│   ├── graphql/        juniper: context, schema, query
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
