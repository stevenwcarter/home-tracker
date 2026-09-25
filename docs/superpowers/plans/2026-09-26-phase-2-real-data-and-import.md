# Phase 2: Real Data and Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the phase-1 placeholder with the real inventory schema, read-side services, the full GraphQL `Query` surface, real statistics, and a Homebox backup importer that loads Steve's actual backup and reproduces Homebox's numbers.

**Architecture:** One Diesel migration creates every inventory table from spec §5. Models are plain Diesel structs, value types (`AssetId`, `Cents`, enum kinds) implement `FromSql`/`ToSql` so bad values cannot enter the database. `svc::*` functions take a `&mut SqliteConnection` and are the only place queries live. GraphQL objects are `#[graphql_object]` impls on the models with relationship resolvers that borrow a pooled connection from the context. The importer reads a `Source` (zip or directory) into serde structs, then upserts in dependency order inside one transaction, copying originals into `$DATA_DIR/originals/<sha256>` and Homebox thumbnails into the `thumbnails` table as the 500 variant. The frontend is untouched in this phase.

**Tech Stack:** Rust 2024, diesel 2.3 (sqlite), juniper 0.17 (`chrono` feature: `NaiveDate` → GraphQL `LocalDate`, `DateTime<Utc>` → `DateTime`), serde/serde_json, zip 8 (`deflate`), sha2 0.10, hex, image 0.25 (`webp`, `jpeg`, `png`, `gif` decode only), chrono, uuid v7, axum-test.

**Spec:** `docs/superpowers/specs/2026-09-25-home-tracker-design.md` (§3 rows 1–4, 7–13, 15; §5; §6 Query side; §8; §9 storage layout; §13; §14; §15 "Phase 2").

## Global Constraints

- Rust `edition = "2024"`; `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all --check` clean at every commit; commit `Cargo.lock` with any dependency change; OpenSSL-free (`cargo tree -i openssl` / `-i native-tls` empty).
- `site/build/index.html` must exist before any `cargo` command (`just site-placeholder`). The frontend under `site/` is not modified in this phase.
- All ids are UUID `TEXT`; new rows use `uuid::Uuid::now_v7().to_string()`; imported rows keep Homebox's ids verbatim.
- Built-in entity types are seeded by migration with fixed ids `00000000-0000-7000-8000-000000000001` (`Location`, `is_location = 1`) and `00000000-0000-7000-8000-000000000002` (`Item`); the importer maps Homebox `global.location` / `global.item` onto them by name (`Location` / `Item`) and keeps every other type's Homebox id.
- Money: `BIGINT` cents. Homebox floats convert with `(price * 100.0).round() as i64`.
- Dates: `purchase_date`, `sold_date`, `warranty_expires` are `DATE` (`chrono::NaiveDate`), taken as the date part of Homebox's RFC 3339 strings. Timestamps are `TIMESTAMP` (`NaiveDateTime`, UTC), parsed from RFC 3339 with `DateTime::parse_from_rfc3339(...)?.naive_utc()`.
- Enum columns (`attachments.kind`, `entity_fields.kind`, `template_fields.kind`) are `TEXT` backed by Rust enums with `FromSql`/`ToSql`; lowercase storage (`photo`, `manual`, `warranty`, `attachment`, `receipt`; `text`, `number`, `boolean`, `time`). Column names use `kind` (not `type`) and `is_primary` (not `primary`); GraphQL exposes `kind` and `primary`.
- Stats (spec §3 rows 7–8): Total Value = `SUM(purchase_price_cents * quantity)` over non-archived, non-location entities, sold included, rounded to cents; Total Items = non-location, non-archived; Total Locations = location-type, archived included; Total Tags = all tags.
- GraphQL naming: Rust `snake_case` → `camelCase`; booleans without `is` prefix except `isLocation`. Cents are `Int`; values beyond `i32` saturate to `i32::MAX` with a `warn!` log.
- Every `Query` field in spec §6 has at least one integration test in `tests/`.
- Originals live at `$DATA_DIR/originals/<sha256 hex>`; the importer writes a file only if it does not already exist.
- The importer refuses any `manifest.json` whose `schemaVersion` is not `1`, naming the version found. It never deletes rows. It counts and warns about `maintenance_entries` and `notifiers` rows without reading them.
- Steve's real backup (`homebox-backup/`, git-excluded) must import with zero errors and produce `summary` = `{ totalValueCents: 293294, currency: "USD", totalItems: 67, totalLocations: 19, totalTags: 6 }` (computed from the JSON by hand with the Homebox formulas). This is the phase's acceptance check; it is run manually in Task 8, not as a committed test.
- Carry-over minors from the phase-1 ledger that this phase MUST close because it touches the code: add `journal_mode`/`busy_timeout` PRAGMA-value tests (Task 1); fix the `serve` error message that names `data_dir` instead of `data_dir/originals` (Task 7); add a `/graphql`-seam test for the missing-currency error (Task 5); make `MIGRATIONS` private (Task 1).
- Commit messages end with `Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF`.

## Review Focus

1. A Homebox entity whose `entity_children` (parent id) points at an entity missing from the export: the importer must not fail the whole run; it imports the row with `parent_id = NULL` and records a warning naming both ids. Task 6 test.
2. An attachment row whose blob is missing from `attachments/`: import the row anyway? No: a row without bytes is unservable, so skip the row with a warning; the run continues. Task 6 test.
3. A location whose parent chain contains a cycle (possible only from a hand-edited export): `ancestors` and `locationTree` must terminate. Task 3 tests (`ancestors` stops at a visited id; tree builder treats a node whose parent is not a root-reachable location as a root).
4. `summary` on a database with zero entities returns zeros (SQL `SUM` yields NULL). Task 4 test.
5. `search` with `%` or `_` in the query must not act as a wildcard: escape them. Task 3 test.

---

### Task 1: Inventory migration, regenerated schema, PRAGMA tests

**Files:**
- Create: `migrations/2026-09-26-000000_inventory/up.sql`, `migrations/2026-09-26-000000_inventory/down.sql`
- Modify: `src/schema.rs` (regenerate), `src/db.rs`

**Interfaces:**
- Produces: diesel tables `entity_types`, `entities`, `tags`, `tag_entities`, `attachments`, `thumbnails`, `entity_fields`, `entity_templates`, `template_fields` in `crate::schema`, plus the seeded built-in type ids as constants in `src/db.rs`: `pub const LOCATION_TYPE_ID: &str = "00000000-0000-7000-8000-000000000001"; pub const ITEM_TYPE_ID: &str = "00000000-0000-7000-8000-000000000002";`.

- [ ] **Step 1: Write the migration**

`migrations/2026-09-26-000000_inventory/up.sql`:

```sql
CREATE TABLE entity_types (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  icon TEXT,
  is_location BOOLEAN NOT NULL DEFAULT 0,
  default_template_id TEXT REFERENCES entity_templates(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE entities (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  entity_type_id TEXT NOT NULL REFERENCES entity_types(id) ON DELETE RESTRICT,
  parent_id TEXT REFERENCES entities(id) ON DELETE RESTRICT,
  archived BOOLEAN NOT NULL DEFAULT 0,
  asset_id BIGINT NOT NULL DEFAULT 0,
  import_ref TEXT,
  notes TEXT,
  quantity DOUBLE NOT NULL DEFAULT 1,
  insured BOOLEAN NOT NULL DEFAULT 0,
  serial_number TEXT,
  model_number TEXT,
  manufacturer TEXT,
  lifetime_warranty BOOLEAN NOT NULL DEFAULT 0,
  warranty_expires DATE,
  warranty_details TEXT,
  purchase_date DATE,
  purchase_from TEXT,
  purchase_price_cents BIGINT NOT NULL DEFAULT 0,
  sold_date DATE,
  sold_to TEXT,
  sold_price_cents BIGINT NOT NULL DEFAULT 0,
  sold_notes TEXT,
  sync_child_entity_locations BOOLEAN NOT NULL DEFAULT 0,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_entities_name ON entities(name);
CREATE INDEX idx_entities_parent_id ON entities(parent_id);
CREATE INDEX idx_entities_entity_type_id ON entities(entity_type_id);
CREATE INDEX idx_entities_archived ON entities(archived);
CREATE INDEX idx_entities_asset_id ON entities(asset_id);

CREATE TABLE tags (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  color TEXT,
  icon TEXT,
  parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tag_entities (
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  PRIMARY KEY (tag_id, entity_id)
);

CREATE TABLE attachments (
  id TEXT PRIMARY KEY NOT NULL,
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  is_primary BOOLEAN NOT NULL DEFAULT 0,
  title TEXT NOT NULL DEFAULT '',
  mime_type TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  size_bytes BIGINT NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_attachments_entity_id ON attachments(entity_id);
CREATE INDEX idx_attachments_sha256 ON attachments(sha256);

CREATE TABLE thumbnails (
  attachment_id TEXT NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
  size INTEGER NOT NULL,
  mime_type TEXT NOT NULL,
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  data BLOB NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (attachment_id, size)
);

CREATE TABLE entity_fields (
  id TEXT PRIMARY KEY NOT NULL,
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  description TEXT,
  kind TEXT NOT NULL,
  text_value TEXT,
  number_value BIGINT,
  boolean_value BOOLEAN NOT NULL DEFAULT 0,
  time_value TIMESTAMP,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_entity_fields_entity_id ON entity_fields(entity_id);

CREATE TABLE entity_templates (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  notes TEXT,
  default_quantity DOUBLE NOT NULL DEFAULT 1,
  default_insured BOOLEAN NOT NULL DEFAULT 0,
  default_name TEXT,
  default_description TEXT,
  default_manufacturer TEXT,
  default_model_number TEXT,
  default_lifetime_warranty BOOLEAN NOT NULL DEFAULT 0,
  default_warranty_details TEXT,
  include_warranty_fields BOOLEAN NOT NULL DEFAULT 0,
  include_purchase_fields BOOLEAN NOT NULL DEFAULT 0,
  include_sold_fields BOOLEAN NOT NULL DEFAULT 0,
  default_tag_ids TEXT,
  location_id TEXT REFERENCES entities(id) ON DELETE SET NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE template_fields (
  id TEXT PRIMARY KEY NOT NULL,
  template_id TEXT NOT NULL REFERENCES entity_templates(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  description TEXT,
  kind TEXT NOT NULL,
  text_value TEXT,
  number_value BIGINT,
  boolean_value BOOLEAN NOT NULL DEFAULT 0,
  time_value TIMESTAMP,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO entity_types (id, name, is_location) VALUES
  ('00000000-0000-7000-8000-000000000001', 'Location', 1),
  ('00000000-0000-7000-8000-000000000002', 'Item', 0);
```

`down.sql`:

```sql
DROP TABLE template_fields;
DROP TABLE entity_templates;
DROP TABLE entity_fields;
DROP TABLE thumbnails;
DROP TABLE attachments;
DROP TABLE tag_entities;
DROP TABLE tags;
DROP TABLE entities;
DROP TABLE entity_types;
```

- [ ] **Step 2: Regenerate `src/schema.rs`**

Run:

```bash
T=$(mktemp -d) && diesel migration run --database-url "$T/x.sqlite" --migration-dir migrations \
  && diesel print-schema --database-url "$T/x.sqlite" > src/schema.rs && rm -rf "$T" && head -30 src/schema.rs
```

Expected: the file starts with `// @generated automatically by Diesel CLI.` and contains a `diesel::table!` block per table (`attachments`, `entities`, `entity_fields`, `entity_templates`, `entity_types`, `settings`, `tag_entities`, `tags`, `template_fields`, `thumbnails`), `joinable!` lines for the non-self-referential foreign keys, and `allow_tables_to_appear_in_same_query!`. Self-referential keys (`entities.parent_id`, `tags.parent_id`) get no `joinable!`, which is expected. If `diesel print-schema` emits `entity_types -> entity_templates` and `entity_templates -> entities` joinables that create an ambiguity error later, leave them; they are only used when both tables are joined in one query, which this phase never does.

- [ ] **Step 3: Add the type-id constants and the failing PRAGMA tests**

In `src/db.rs` add after `pub type SqlitePool`:

```rust
/// Seeded built-in entity type ids (see the inventory migration). The importer
/// maps Homebox's `global.location` / `global.item` onto these by name.
pub const LOCATION_TYPE_ID: &str = "00000000-0000-7000-8000-000000000001";
pub const ITEM_TYPE_ID: &str = "00000000-0000-7000-8000-000000000002";
```

Change `pub const MIGRATIONS` to `const MIGRATIONS` (phase-1 deferred minor).

Add to the `tests` module in `src/db.rs`:

```rust
    #[test]
    fn wal_and_busy_timeout_are_set_on_every_connection() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let mode = diesel::sql_query("PRAGMA journal_mode")
            .get_result::<JournalMode>(&mut conn)
            .unwrap();
        assert_eq!(mode.journal_mode.to_lowercase(), "wal");
        let timeout = diesel::sql_query("PRAGMA busy_timeout")
            .get_result::<BusyTimeout>(&mut conn)
            .unwrap();
        assert_eq!(timeout.timeout, 5000);
    }

    #[test]
    fn built_in_types_are_seeded() {
        use crate::schema::entity_types::dsl::*;
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let rows: Vec<(String, String, bool)> = entity_types
            .select((id, name, is_location))
            .order(name.asc())
            .load(&mut conn)
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (ITEM_TYPE_ID.to_owned(), "Item".to_owned(), false),
                (LOCATION_TYPE_ID.to_owned(), "Location".to_owned(), true),
            ]
        );
    }

    #[derive(QueryableByName)]
    struct JournalMode {
        #[diesel(sql_type = diesel::sql_types::Text)]
        journal_mode: String,
    }

    #[derive(QueryableByName)]
    struct BusyTimeout {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        timeout: i32,
    }
```

- [ ] **Step 4: Run, verify, commit**

Run: `cargo test db:: && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: the two new tests pass; everything else still passes (the phase-1 `settings` tests and integration tests are unaffected).

```bash
git add migrations src/schema.rs src/db.rs
git commit -m "feat: inventory schema migration with seeded built-in types

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 2: Value types and enum columns

**Files:**
- Create: `src/asset_id.rs`, `src/money.rs`, `src/kinds.rs`
- Modify: `src/lib.rs` (add `pub mod asset_id; pub mod kinds; pub mod money;`)

**Interfaces:**
- Produces: `AssetId(pub i64)` with `AssetId::NONE`, `is_none()`, `Display` (`000-007`, empty string when none), `FromStr` accepting `000-007`, `7`, `"000007"`; `FromSql<BigInt, Sqlite>` / `ToSql<BigInt, Sqlite>`; `AssetId::display_option(&self) -> Option<String>` (None when none).
- Produces: `Cents(pub i64)` with `Cents::from_major_f64(f64) -> Cents` (round half away from zero), `as_graphql_int(&self) -> i32` (saturating with `warn!`), `FromSql`/`ToSql` over `BigInt`, `Default` = 0, arithmetic via plain `.0`.
- Produces: `AttachmentKind { Photo, Manual, Warranty, Attachment, Receipt }`, `FieldKind { Text, Number, Boolean, Time }`: `as_str()`, `FromStr` (case-insensitive), `FromSql<Text, Sqlite>`/`ToSql<Text, Sqlite>`, `juniper::GraphQLEnum` (GraphQL names `PHOTO`, … , `TEXT`, …), `AttachmentKind::is_thumbnail_marker(s: &str) -> bool` is NOT provided; the importer compares the raw string `"thumbnail"` itself.

- [ ] **Step 1: Write the failing tests and the types**

`src/asset_id.rs`:

```rust
//! Homebox-style asset numbers, shown as `000-007`. Zero means "no asset id".

use std::fmt;
use std::str::FromStr;

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::BigInt;
use diesel::sqlite::{Sqlite, SqliteValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, AsExpression, FromSqlRow)]
#[diesel(sql_type = BigInt)]
pub struct AssetId(pub i64);

impl AssetId {
    pub const NONE: Self = Self(0);

    pub fn is_none(self) -> bool {
        self.0 <= 0
    }

    /// `Some("000-007")`, or `None` when there is no asset id.
    pub fn display_option(self) -> Option<String> {
        (!self.is_none()).then(|| self.to_string())
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() {
            return Ok(());
        }
        let digits = format!("{:06}", self.0);
        let (head, tail) = digits.split_at(digits.len() - 3);
        write!(f, "{head}-{tail}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid asset id {0:?}: expected digits, optionally as 000-000")]
pub struct ParseAssetIdError(String);

impl FromStr for AssetId {
    type Err = ParseAssetIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let cleaned: String = s.trim().chars().filter(|c| *c != '-').collect();
        if cleaned.is_empty() {
            return Ok(Self::NONE);
        }
        cleaned
            .parse::<i64>()
            .map(Self)
            .map_err(|_| ParseAssetIdError(s.to_owned()))
    }
}

impl FromSql<BigInt, Sqlite> for AssetId {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(Self(<i64 as FromSql<BigInt, Sqlite>>::from_sql(value)?))
    }
}

impl ToSql<BigInt, Sqlite> for AssetId {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0);
        Ok(IsNull::No)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_homebox() {
        assert_eq!(AssetId(7).to_string(), "000-007");
        assert_eq!(AssetId(123456).to_string(), "123-456");
        assert_eq!(AssetId(1234567).to_string(), "1234-567");
        assert_eq!(AssetId::NONE.to_string(), "");
        assert_eq!(AssetId(-3).display_option(), None);
        assert_eq!(AssetId(7).display_option().as_deref(), Some("000-007"));
    }

    #[test]
    fn parses_dashed_plain_and_empty() {
        assert_eq!("000-007".parse::<AssetId>().unwrap(), AssetId(7));
        assert_eq!("7".parse::<AssetId>().unwrap(), AssetId(7));
        assert_eq!(" 000007 ".parse::<AssetId>().unwrap(), AssetId(7));
        assert_eq!("".parse::<AssetId>().unwrap(), AssetId::NONE);
        assert!("abc".parse::<AssetId>().is_err());
    }
}
```

Add `thiserror = "2"` to `[dependencies]` in `Cargo.toml` (library error types per the crate-decisions file).

`src/money.rs`:

```rust
//! Money as integer minor units. The currency code lives in `settings`.

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::BigInt;
use diesel::sqlite::{Sqlite, SqliteValue};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, AsExpression, FromSqlRow)]
#[diesel(sql_type = BigInt)]
pub struct Cents(pub i64);

impl Cents {
    /// Converts a major-unit float (Homebox stores `1099.99`) to cents, rounding
    /// half away from zero.
    pub fn from_major_f64(major: f64) -> Self {
        Self((major * 100.0).round() as i64)
    }

    /// GraphQL `Int` is 32-bit; anything larger saturates and is logged.
    pub fn as_graphql_int(self) -> i32 {
        i32::try_from(self.0).unwrap_or_else(|_| {
            warn!(cents = self.0, "cents value exceeds GraphQL Int; saturating");
            if self.0 < 0 { i32::MIN } else { i32::MAX }
        })
    }
}

impl FromSql<BigInt, Sqlite> for Cents {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(Self(<i64 as FromSql<BigInt, Sqlite>>::from_sql(value)?))
    }
}

impl ToSql<BigInt, Sqlite> for Cents {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0);
        Ok(IsNull::No)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_homebox_floats_to_cents() {
        assert_eq!(Cents::from_major_f64(1099.99), Cents(109_999));
        assert_eq!(Cents::from_major_f64(0.0), Cents(0));
        assert_eq!(Cents::from_major_f64(152.99), Cents(15_299));
        assert_eq!(Cents::from_major_f64(0.005), Cents(1));
        assert_eq!(Cents::from_major_f64(-2.5), Cents(-250));
    }

    #[test]
    fn saturates_for_graphql() {
        assert_eq!(Cents(293_294).as_graphql_int(), 293_294);
        assert_eq!(Cents(i64::MAX).as_graphql_int(), i32::MAX);
        assert_eq!(Cents(i64::MIN).as_graphql_int(), i32::MIN);
    }
}
```

`src/kinds.rs`:

```rust
//! Closed string enums stored as lowercase TEXT.

use std::fmt;
use std::str::FromStr;

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::Text;
use diesel::sqlite::{Sqlite, SqliteValue};
use juniper::GraphQLEnum;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} {value:?}")]
pub struct UnknownKind {
    kind: &'static str,
    value: String,
}

macro_rules! text_enum {
    ($name:ident, $label:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsExpression, FromSqlRow, GraphQLEnum)]
        #[diesel(sql_type = Text)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = UnknownKind;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.trim().to_ascii_lowercase().as_str() {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(UnknownKind { kind: $label, value: s.to_owned() }),
                }
            }
        }

        impl FromSql<Text, Sqlite> for $name {
            fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
                let raw = <String as FromSql<Text, Sqlite>>::from_sql(value)?;
                Ok(raw.parse()?)
            }
        }

        impl ToSql<Text, Sqlite> for $name {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
                out.set_value(self.as_str());
                Ok(IsNull::No)
            }
        }
    };
}

text_enum!(AttachmentKind, "attachment kind", {
    Photo => "photo",
    Manual => "manual",
    Warranty => "warranty",
    Attachment => "attachment",
    Receipt => "receipt",
});

text_enum!(FieldKind, "field kind", {
    Text => "text",
    Number => "number",
    Boolean => "boolean",
    Time => "time",
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_variant_through_text() {
        for kind in AttachmentKind::ALL {
            assert_eq!(kind.as_str().parse::<AttachmentKind>().unwrap(), *kind);
        }
        for kind in FieldKind::ALL {
            assert_eq!(kind.as_str().parse::<FieldKind>().unwrap(), *kind);
        }
    }

    #[test]
    fn parsing_is_case_insensitive_and_rejects_unknowns() {
        assert_eq!("PHOTO".parse::<AttachmentKind>().unwrap(), AttachmentKind::Photo);
        assert_eq!(" Time ".parse::<FieldKind>().unwrap(), FieldKind::Time);
        assert!("thumbnail".parse::<AttachmentKind>().is_err());
        assert!("json".parse::<FieldKind>().is_err());
    }
}
```

- [ ] **Step 2: Run tests, verify they pass, and check a SQL round-trip**

Add one more test to `src/kinds.rs` `tests` that proves the `FromSql`/`ToSql` wiring against a real table (this is the seam later tasks rely on):

```rust
    #[test]
    fn kinds_round_trip_through_sqlite() {
        use crate::db::{ITEM_TYPE_ID, TestDb};
        use crate::schema::{attachments, entities};
        use diesel::prelude::*;

        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::insert_into(entities::table)
            .values((
                entities::id.eq("e1"),
                entities::name.eq("Thing"),
                entities::entity_type_id.eq(ITEM_TYPE_ID),
            ))
            .execute(&mut conn)
            .unwrap();
        diesel::insert_into(attachments::table)
            .values((
                attachments::id.eq("a1"),
                attachments::entity_id.eq("e1"),
                attachments::kind.eq(AttachmentKind::Receipt),
                attachments::mime_type.eq("application/pdf"),
                attachments::sha256.eq("00"),
                attachments::size_bytes.eq(2_i64),
            ))
            .execute(&mut conn)
            .unwrap();
        let (stored_text, stored_kind): (String, AttachmentKind) = attachments::table
            .select((
                diesel::dsl::sql::<diesel::sql_types::Text>("kind"),
                attachments::kind,
            ))
            .first(&mut conn)
            .unwrap();
        assert_eq!(stored_text, "receipt");
        assert_eq!(stored_kind, AttachmentKind::Receipt);
    }
```

Run: `cargo test asset_id money kinds && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass. If `Ok(raw.parse()?)` fails to convert `UnknownKind` into `deserialize::Result`'s boxed error, use `raw.parse().map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)`.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/asset_id.rs src/money.rs src/kinds.rs
git commit -m "feat: AssetId, Cents and kind enums with sql round-trips

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 3: Models, sample fixture, and read services

**Files:**
- Create: `src/models/mod.rs`, `src/models/entity_type.rs`, `src/models/entity.rs`, `src/models/tag.rs`, `src/models/attachment.rs`, `src/models/thumbnail.rs`, `src/models/entity_field.rs`, `src/models/entity_template.rs`, `src/models/template_field.rs`, `src/svc/fixtures.rs`, `src/svc/entity_type.rs`, `src/svc/entity.rs`, `src/svc/tag.rs`, `src/svc/attachment.rs`, `src/svc/entity_field.rs`
- Modify: `src/lib.rs` (`pub mod models;`), `src/svc/mod.rs`

**Interfaces:**
- Produces models (all `#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]`, `#[diesel(table_name = ..., check_for_backend(Sqlite))]`, field names identical to the columns, types: `String` ids, `Option<String>` for nullable text, `bool`, `f64` for `DOUBLE`, `i64` for plain `BIGINT`, `AssetId` for `entities.asset_id`, `Cents` for the two `*_cents` columns, `AttachmentKind`/`FieldKind` for `kind`, `NaiveDate` for `DATE`, `NaiveDateTime` for `TIMESTAMP`, `Vec<u8>` for `BLOB`):
  `EntityType`, `Entity`, `Tag`, `TagEntity { tag_id, entity_id }` (Insertable + Queryable only), `Attachment`, `Thumbnail`, `EntityField`, `EntityTemplate`, `TemplateField`.
- Produces `svc::fixtures::seed_sample(conn: &mut SqliteConnection) -> SampleIds` (public, used by integration tests) that inserts: types Location/Item (seeded) plus `Tote` (location, id `t-tote`); entities `House` (location, root), `Garage` (location, parent House), `Tote A` (Tote, parent Garage), `Attic` (location, root, archived), `Drill` (item, parent Garage, price 15299, qty 1, asset 3, tags [Tools]), `Screws` (item, parent Tote A, price 999, qty 2, asset 4), `Old TV` (item, parent House, price 50000, sold_price 20000, sold_date set, asset 5), `Broken lamp` (item, parent House, archived, price 1000), `Loose item` (item, no parent, price 0); tags `Tools`, `Electronics` (parent Tools); attachments on Drill: one `photo` primary (sha `aa..`, 3 bytes) and one `manual` (sha `bb..`); one thumbnail row for the photo at size 500 (width 4, height 3, 8 bytes); one custom field on Drill (`Voltage`, Number, 18). Returns `SampleIds { house, garage, tote_a, attic, drill, screws, old_tv, broken_lamp, loose, tools, electronics, photo, manual, tote_type }` (all `String`).
- Produces `svc::entity_type`: `list(conn) -> Result<Vec<EntityType>>` (ordered by name), `get(conn, id: &str) -> Result<Option<EntityType>>`, `entity_count(conn, type_id: &str) -> Result<i64>`.
- Produces `svc::entity`: `get(conn, id) -> Result<Option<Entity>>`, `is_location(conn, entity: &Entity) -> Result<bool>`, `children(conn, parent_id) -> Result<Vec<Entity>>` (ordered by name, case-insensitive: `.order(lower(name))` via `diesel::dsl::sql`), `child_locations(conn, parent_id) -> Result<Vec<Entity>>` and `items(conn, parent_id) -> Result<Vec<Entity>>` (children filtered by the type's `is_location` via `inner_join(entity_types::table)`), `root_items(conn) -> Result<Vec<Entity>>` (non-location, `parent_id IS NULL`), `ancestors(conn, id) -> Result<Vec<Entity>>` (root first; stops at a visited id or after 64 hops), `location_tree(conn) -> Result<Vec<LocationNode>>` with `pub struct LocationNode { pub entity: Entity, pub children: Vec<LocationNode> }`, `search(conn, query: &str, limit: i64) -> Result<Vec<Entity>>` (escaped `LIKE`, `ESCAPE '\'`, non-archived first then name).
- Produces `svc::tag`: `list(conn) -> Result<Vec<Tag>>`, `get(conn, id) -> Result<Option<Tag>>`, `for_entity(conn, entity_id) -> Result<Vec<Tag>>`, `entity_count(conn, tag_id) -> Result<i64>`.
- Produces `svc::attachment`: `for_entity(conn, entity_id) -> Result<Vec<Attachment>>` (primary first, then created_at), `primary_photo(conn, entity_id) -> Result<Option<Attachment>>` (kind photo, is_primary; falls back to the earliest photo), `get(conn, id) -> Result<Option<Attachment>>`, `thumbnail(conn, attachment_id, size: i32) -> Result<Option<Thumbnail>>`.
- Produces `svc::entity_field::for_entity(conn, entity_id) -> Result<Vec<EntityField>>`.

- [ ] **Step 1: Write the models**

`src/models/mod.rs` re-exports every model: `pub use attachment::Attachment; pub use entity::Entity; …`. Example `src/models/entity.rs` (the others follow the same shape with their own columns):

```rust
use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;

use crate::asset_id::AssetId;
use crate::money::Cents;
use crate::schema::entities;

#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = entities, check_for_backend(diesel::sqlite::Sqlite))]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub entity_type_id: String,
    pub parent_id: Option<String>,
    pub archived: bool,
    pub asset_id: AssetId,
    pub import_ref: Option<String>,
    pub notes: Option<String>,
    pub quantity: f64,
    pub insured: bool,
    pub serial_number: Option<String>,
    pub model_number: Option<String>,
    pub manufacturer: Option<String>,
    pub lifetime_warranty: bool,
    pub warranty_expires: Option<NaiveDate>,
    pub warranty_details: Option<String>,
    pub purchase_date: Option<NaiveDate>,
    pub purchase_from: Option<String>,
    pub purchase_price_cents: Cents,
    pub sold_date: Option<NaiveDate>,
    pub sold_to: Option<String>,
    pub sold_price_cents: Cents,
    pub sold_notes: Option<String>,
    pub sync_child_entity_locations: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
```

`#[derive(AsChangeset)]` with `Option<String>` fields treats `None` as "skip"; that is acceptable for phase 2 (only the importer writes, and it writes whole rows via `Insertable`). Add `#[diesel(treat_none_as_null = true)]` on every model so an update with `None` really nulls the column; phase 4's mutations depend on that.

Field order must match the column order in `schema.rs` for `Queryable`; `Selectable` + `.select(Entity::as_select())` makes reads order-independent, so every query in `svc` uses `as_select()`.

- [ ] **Step 2: Write the sample fixture**

`src/svc/fixtures.rs`: a single `pub fn seed_sample(conn: &mut SqliteConnection) -> SampleIds` inserting the rows listed in Interfaces with `diesel::insert_into(...).values(...)` tuples, fixed readable ids (`"e-house"`, `"e-garage"`, `"e-tote-a"`, `"e-attic"`, `"e-drill"`, `"e-screws"`, `"e-old-tv"`, `"e-broken-lamp"`, `"e-loose"`, `"t-tools"`, `"t-electronics"`, `"a-drill-photo"`, `"a-drill-manual"`, `"t-tote"`), `created_at` values spaced one second apart so ordering tests are deterministic, and `sold_date = 2025-01-15` for Old TV. Old TV's `sold_price_cents` is 20000; its `purchase_price_cents` is 50000. This fixture is the single source used by the service tests here, the stats test (Task 4), and the GraphQL tests (Task 5). Its expected statistics: Total Value = 15299×1 + 999×2 + 50000×1 = 67297 (Broken lamp archived, Loose item 0, locations excluded), Total Items = 4 (Drill, Screws, Old TV, Loose), Total Locations = 4 (House, Garage, Tote A, Attic), Total Tags = 2.

- [ ] **Step 3: Write the failing service tests**

Per service module, a `#[cfg(test)] mod tests` using `TestDb` + `seed_sample`. Required cases (each is one `#[test]`):

`svc::entity_type`: `list_is_ordered_by_name` (Item, Location, Tote); `entity_count_counts_all_entities_of_the_type` (Location → 3: House, Garage, Attic; Tote → 1).

`svc::entity`:
- `children_are_sorted_case_insensitively` (insert an extra child `apple` and `Banana` under House; order is `apple`, `Banana`, `Broken lamp`, `Garage`, `Old TV`).
- `child_locations_and_items_split_by_type` (Garage → child_locations [Tote A], items [Drill]).
- `root_items_are_parentless_non_locations` (→ [Loose item]).
- `ancestors_are_root_first` (Tote A → [House, Garage]).
- `ancestors_terminate_on_a_cycle` (Review Focus 3: `UPDATE entities SET parent_id = 'e-tote-a' WHERE id = 'e-house'`; `ancestors('e-tote-a')` returns without hanging and contains each id at most once).
- `location_tree_nests_locations_only` (roots [Attic, House] sorted by name; House → [Garage] → [Tote A]; no items anywhere in the tree; archived Attic included).
- `location_tree_treats_an_orphaned_location_as_a_root` (set Garage's parent to a non-existent id via `PRAGMA foreign_keys=OFF` in the test, then Garage appears as a root).
- `search_matches_name_case_insensitively_and_escapes_wildcards` (Review Focus 5: `search("dRiLl")` → [Drill]; `search("%")` → empty; `search("_")` → empty; insert an entity named `100% cotton` and `search("100%")` finds only it).
- `search_lists_non_archived_first` (insert `Zeta` archived and `Zeta` active; active first).

`svc::tag`: `for_entity_returns_assigned_tags` (Drill → [Tools]); `entity_count` (Tools → 1, Electronics → 0).

`svc::attachment`: `for_entity_lists_primary_first` (Drill → [photo, manual]); `primary_photo_prefers_the_flag_then_the_earliest_photo` (flag case from the fixture; then clear the flag and insert a second, later photo, expect the earliest); `thumbnail_returns_the_stored_size_only` (500 → Some with width 4; 300 → None).

`svc::entity_field`: `for_entity_returns_custom_fields` (Drill → [Voltage] with `number_value = Some(18)`).

- [ ] **Step 4: Run to see them fail, implement the services, run to see them pass**

Implementation notes that the code must follow:

- `children`: `entities::table.filter(entities::parent_id.eq(parent)).order(diesel::dsl::sql::<diesel::sql_types::Text>("lower(name)")).select(Entity::as_select()).load(conn)`.
- `child_locations` / `items`: `entities::table.inner_join(entity_types::table).filter(entities::parent_id.eq(parent)).filter(entity_types::is_location.eq(true /* or false */)).order(sql("lower(entities.name)")).select(Entity::as_select()).load(conn)`.
- `ancestors`: loop `get(parent_id)`, `HashSet<String>` of visited ids, `const MAX_DEPTH: usize = 64`; reverse at the end.
- `location_tree`: load all location entities with `inner_join` on `is_location = true`, sort by `name.to_lowercase()`, build `HashMap<Option<String>, Vec<Entity>>` keyed by parent, where a parent id that is not itself in the location set is normalised to `None` (orphans become roots); recurse with a visited set so a cycle cannot recurse forever.
- `search`: `fn escape_like(q: &str) -> String` replaces `\` → `\\`, `%` → `\%`, `_` → `\_`; query `entities::name.like(format!("%{}%", escape_like(q))).escape('\\')`, `.order((entities::archived.asc(), sql("lower(name)")))`, `.limit(limit)`.
- `primary_photo`: first try `kind = Photo AND is_primary`, else `kind = Photo ORDER BY created_at ASC LIMIT 1`.

Run: `cargo test svc:: && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all listed tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/models src/svc
git commit -m "feat: inventory models, sample fixture and read services

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 4: Real statistics

**Files:**
- Modify: `src/svc/stats.rs`

**Interfaces:**
- Produces: unchanged `Summary` shape and `summary(conn)`; `total_value_cents` now computed.

- [ ] **Step 1: Replace the placeholder tests with characterization tests**

Replace the `tests` module in `src/svc/stats.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;
    use crate::schema::settings as settings_table;
    use crate::svc::fixtures::seed_sample;
    use diesel::prelude::*;

    #[test]
    fn summary_matches_the_homebox_formulas_on_the_sample() {
        // Value = Drill 15299×1 + Screws 999×2 + Old TV 50000×1 (sold, still counted).
        // Broken lamp is archived (excluded), Loose item is 0, locations excluded.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        seed_sample(&mut conn);
        let s = summary(&mut conn).unwrap();
        assert_eq!(
            s,
            Summary {
                total_value_cents: 67_297,
                currency: "USD".to_owned(),
                total_items: 4,
                total_locations: 4,
                total_tags: 2,
            }
        );
    }

    #[test]
    fn an_empty_database_reports_zeros() {
        // Review focus 4: SUM over no rows is NULL in SQLite.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let s = summary(&mut conn).unwrap();
        assert_eq!((s.total_value_cents, s.total_items, s.total_locations, s.total_tags), (0, 0, 0, 0));
    }

    #[test]
    fn fractional_quantities_round_to_cents() {
        use crate::db::ITEM_TYPE_ID;
        use crate::schema::entities;
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::insert_into(entities::table)
            .values((
                entities::id.eq("e-half"),
                entities::name.eq("Half"),
                entities::entity_type_id.eq(ITEM_TYPE_ID),
                entities::quantity.eq(1.5),
                entities::purchase_price_cents.eq(crate::money::Cents(333)),
            ))
            .execute(&mut conn)
            .unwrap();
        assert_eq!(summary(&mut conn).unwrap().total_value_cents, 500); // 499.5 rounds half away from zero
    }

    #[test]
    fn summary_reads_the_currency_from_settings() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::update(settings_table::table.filter(settings_table::key.eq("currency")))
            .set(settings_table::value.eq("EUR"))
            .execute(&mut conn)
            .unwrap();
        assert_eq!(summary(&mut conn).unwrap().currency, "EUR");
    }

    #[test]
    fn summary_fails_when_the_currency_row_is_gone() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::delete(settings_table::table).execute(&mut conn).unwrap();
        assert!(summary(&mut conn).is_err());
    }
}
```

- [ ] **Step 2: Run to verify the first three fail, then implement**

Replace `summary` and add a raw-SQL row type:

```rust
#[derive(QueryableByName)]
struct ValueRow {
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
    total: Option<f64>,
}

/// Homebox's formulas (spec §3 rows 7 and 8): value counts sold items and
/// multiplies by quantity; archived items are excluded from value and item
/// count; locations are counted whether archived or not.
pub fn summary(conn: &mut SqliteConnection) -> Result<Summary> {
    use crate::schema::{entities, entity_types, tags};

    let currency = settings::currency(conn)?;

    let non_location_active = entities::table
        .inner_join(entity_types::table)
        .filter(entity_types::is_location.eq(false))
        .filter(entities::archived.eq(false));

    let total_items: i64 = non_location_active.count().get_result(conn)?;
    let total_locations: i64 = entities::table
        .inner_join(entity_types::table)
        .filter(entity_types::is_location.eq(true))
        .count()
        .get_result(conn)?;
    let total_tags: i64 = tags::table.count().get_result(conn)?;

    let value: ValueRow = diesel::sql_query(
        "SELECT SUM(e.purchase_price_cents * e.quantity) AS total \
         FROM entities e JOIN entity_types t ON t.id = e.entity_type_id \
         WHERE t.is_location = 0 AND e.archived = 0",
    )
    .get_result(conn)
    .context("computing total value")?;
    let total_value_cents = Cents(value.total.unwrap_or(0.0).round() as i64);

    Ok(Summary {
        total_value_cents: total_value_cents.as_graphql_int(),
        currency,
        total_items: i32::try_from(total_items).unwrap_or(i32::MAX),
        total_locations: i32::try_from(total_locations).unwrap_or(i32::MAX),
        total_tags: i32::try_from(total_tags).unwrap_or(i32::MAX),
    })
}
```

(`use anyhow::Context as _; use crate::money::Cents;` at the top.) Keep `Summary` as it is.

Run: `cargo test svc::stats && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: the phase-1 integration test `tests/graphql_summary.rs::summary_query_returns_the_placeholder_numbers_and_currency` now FAILS because it asserts the placeholders. Update it in this task: seed the sample with `home_tracker::svc::fixtures::seed_sample` and assert `67297 / "USD" / 4 / 4 / 2`; rename it `summary_query_reports_the_sample_statistics`. Then all green.

- [ ] **Step 3: Commit**

```bash
git add src/svc/stats.rs tests/graphql_summary.rs
git commit -m "feat: compute the home-page statistics with Homebox's formulas

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 5: GraphQL objects and the full Query surface

**Files:**
- Create: `src/graphql/objects/mod.rs`, `src/graphql/objects/entity_type.rs`, `src/graphql/objects/entity.rs`, `src/graphql/objects/tag.rs`, `src/graphql/objects/attachment.rs`, `src/graphql/objects/entity_field.rs`, `src/graphql/objects/location_node.rs`, `tests/graphql_queries.rs`
- Modify: `src/graphql/mod.rs`, `src/graphql/context.rs`, `src/graphql/query.rs`, `tests/graphql_summary.rs`

**Interfaces:**
- Consumes: every `svc::*` function from Tasks 3–4, `AssetId::display_option`, `Cents::as_graphql_int`.
- Produces: `GraphQLContext::conn(&self) -> anyhow::Result<PooledConnection<ConnectionManager<SqliteConnection>>>`.
- Produces GraphQL types exactly as spec §6 (Query side): `EntityType`, `Entity`, `Tag`, `Attachment`, `AttachmentKind`, `EntityField`, `FieldKind`, `LocationNode`, and `Query { summary, entityTypes, locationTree, entity(id), rootItems, tags, search(query, limit = 25) }`. Scalars: `LocalDate` for dates, `DateTime` for timestamps (juniper's chrono names), `Float` for `quantity`, `Int` for cents.
- `Attachment.url` = `/attachments/{id}`; `Attachment.thumbnailUrl(size: Int = 500)` = `Some("/attachments/{id}/thumb/{size}")` when `mime_type` starts with `image/`, else `None`. (The HTTP handlers arrive in phase 3; the URLs are the contract.)

- [ ] **Step 1: Add `conn()` to the context and write the objects**

`src/graphql/context.rs`, in `impl GraphQLContext`:

```rust
    /// One pooled connection for the duration of a resolver.
    pub fn conn(&self) -> anyhow::Result<PooledConnection<ConnectionManager<SqliteConnection>>> {
        self.pool.get().context("could not get a database connection")
    }
```

(`use anyhow::Context as _; use diesel::SqliteConnection; use diesel::r2d2::{ConnectionManager, PooledConnection};`.)

`src/graphql/objects/entity.rs` (the pattern every object follows):

```rust
use chrono::{DateTime, NaiveDate, Utc};
use juniper::FieldResult;

use crate::graphql::context::GraphQLContext;
use crate::graphql::schema::graphql_translate_anyhow as gql;
use crate::models::{Attachment, Entity, EntityField, EntityType, Tag};
use crate::svc;

#[juniper::graphql_object(context = GraphQLContext, name = "Entity")]
impl Entity {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { self.description.as_deref() }
    fn entity_type(&self, ctx: &GraphQLContext) -> FieldResult<EntityType> {
        gql(ctx.conn().and_then(|mut c| {
            svc::entity_type::get(&mut c, &self.entity_type_id)?
                .ok_or_else(|| anyhow::anyhow!("entity type {} missing", self.entity_type_id))
        }))
    }
    fn is_location(&self, ctx: &GraphQLContext) -> FieldResult<bool> {
        gql(ctx.conn().and_then(|mut c| svc::entity::is_location(&mut c, self)))
    }
    fn parent(&self, ctx: &GraphQLContext) -> FieldResult<Option<Entity>> {
        match &self.parent_id {
            None => Ok(None),
            Some(pid) => gql(ctx.conn().and_then(|mut c| svc::entity::get(&mut c, pid))),
        }
    }
    fn ancestors(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(ctx.conn().and_then(|mut c| svc::entity::ancestors(&mut c, &self.id)))
    }
    fn child_locations(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(ctx.conn().and_then(|mut c| svc::entity::child_locations(&mut c, &self.id)))
    }
    fn items(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Entity>> {
        gql(ctx.conn().and_then(|mut c| svc::entity::items(&mut c, &self.id)))
    }
    fn archived(&self) -> bool { self.archived }
    fn asset_id(&self) -> Option<String> { self.asset_id.display_option() }
    fn quantity(&self) -> f64 { self.quantity }
    fn insured(&self) -> bool { self.insured }
    fn serial_number(&self) -> Option<&str> { self.serial_number.as_deref() }
    fn model_number(&self) -> Option<&str> { self.model_number.as_deref() }
    fn manufacturer(&self) -> Option<&str> { self.manufacturer.as_deref() }
    fn notes(&self) -> Option<&str> { self.notes.as_deref() }
    fn lifetime_warranty(&self) -> bool { self.lifetime_warranty }
    fn warranty_expires(&self) -> Option<NaiveDate> { self.warranty_expires }
    fn warranty_details(&self) -> Option<&str> { self.warranty_details.as_deref() }
    fn purchase_date(&self) -> Option<NaiveDate> { self.purchase_date }
    fn purchase_from(&self) -> Option<&str> { self.purchase_from.as_deref() }
    fn purchase_price_cents(&self) -> i32 { self.purchase_price_cents.as_graphql_int() }
    fn sold_date(&self) -> Option<NaiveDate> { self.sold_date }
    fn sold_to(&self) -> Option<&str> { self.sold_to.as_deref() }
    fn sold_price_cents(&self) -> i32 { self.sold_price_cents.as_graphql_int() }
    fn sold_notes(&self) -> Option<&str> { self.sold_notes.as_deref() }
    fn tags(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Tag>> {
        gql(ctx.conn().and_then(|mut c| svc::tag::for_entity(&mut c, &self.id)))
    }
    fn attachments(&self, ctx: &GraphQLContext) -> FieldResult<Vec<Attachment>> {
        gql(ctx.conn().and_then(|mut c| svc::attachment::for_entity(&mut c, &self.id)))
    }
    fn primary_photo(&self, ctx: &GraphQLContext) -> FieldResult<Option<Attachment>> {
        gql(ctx.conn().and_then(|mut c| svc::attachment::primary_photo(&mut c, &self.id)))
    }
    fn fields(&self, ctx: &GraphQLContext) -> FieldResult<Vec<EntityField>> {
        gql(ctx.conn().and_then(|mut c| svc::entity_field::for_entity(&mut c, &self.id)))
    }
    fn created_at(&self) -> DateTime<Utc> { self.created_at.and_utc() }
    fn updated_at(&self) -> DateTime<Utc> { self.updated_at.and_utc() }
}
```

`EntityType` object: `id, name, description, icon, is_location, entity_count (svc::entity_type::entity_count → i32), created_at, updated_at`. `Tag`: `id, name, description, color, icon, parent (svc::tag::get), entity_count`. `Attachment`: `id, kind, primary (self.is_primary), title, mime_type, size_bytes (i32 saturating), url, thumbnail_url(size: Option<i32>)` with `#[graphql(default = 500)]` on `size` if juniper's `default` attribute is available for method args, otherwise `size: Option<i32>` and `.unwrap_or(500)`. `EntityField`: `id, name, kind, text_value, number_value (i32 saturating), boolean_value, time_value (DateTime<Utc>)`. `LocationNode`: `#[derive(GraphQLObject)] #[graphql(context = GraphQLContext)] pub struct LocationNode { pub entity: Entity, pub children: Vec<LocationNode> }` built with a `From<svc::entity::LocationNode>` conversion (or reuse the svc struct directly if juniper accepts the derive on it; prefer reuse to avoid a copy).

`src/graphql/query.rs` gains:

```rust
    fn entity_types(context: &GraphQLContext) -> FieldResult<Vec<EntityType>> { … svc::entity_type::list }
    fn location_tree(context: &GraphQLContext) -> FieldResult<Vec<LocationNode>> { … svc::entity::location_tree }
    fn entity(context: &GraphQLContext, id: String) -> FieldResult<Option<Entity>> { … svc::entity::get }
    fn root_items(context: &GraphQLContext) -> FieldResult<Vec<Entity>> { … svc::entity::root_items }
    fn tags(context: &GraphQLContext) -> FieldResult<Vec<Tag>> { … svc::tag::list }
    fn search(context: &GraphQLContext, query: String, limit: Option<i32>) -> FieldResult<Vec<Entity>> {
        let limit = i64::from(limit.unwrap_or(25).clamp(1, 200));
        … svc::entity::search(&mut c, &query, limit)
    }
```

and `summary` switches to `context.conn()`.

- [ ] **Step 2: Write the failing integration tests**

`tests/graphql_queries.rs` uses a helper:

```rust
use axum_test::TestServer;
use home_tracker::db::TestDb;
use home_tracker::routes::app;
use home_tracker::svc::fixtures::{SampleIds, seed_sample};
use serde_json::{Value, json};

async fn seeded() -> (TestServer, SampleIds, TestDb) {
    let db = TestDb::new();
    let ids = seed_sample(&mut db.pool.get().unwrap());
    let server = TestServer::new(app(db.pool.clone()));
    (server, ids, db)
}

async fn query(server: &TestServer, q: &str, vars: Value) -> Value {
    let r = server.post("/graphql").json(&json!({ "query": q, "variables": vars })).await;
    r.assert_status_ok();
    let body: Value = r.json();
    assert!(body.get("errors").is_none(), "unexpected errors: {body}");
    body["data"].clone()
}
```

One `#[tokio::test]` per query field:

- `entity_types_lists_all_with_counts`: `{ entityTypes { name isLocation entityCount } }` → three rows in name order, `Location` has `entityCount: 3`.
- `location_tree_nests_locations`: `{ locationTree { entity { name } children { entity { name } children { entity { name } } } } }` → roots `Attic`, `House`; `House.children[0].entity.name == "Garage"`, whose child is `Tote A`.
- `entity_returns_every_scalar_and_relationship`: query Drill with every field in the spec; assert `assetId == "000-003"`, `purchasePriceCents == 15299`, `isLocation == false`, `entityType.name == "Item"`, `parent.name == "Garage"`, `ancestors` names `["House","Garage"]`, `tags[0].name == "Tools"`, `attachments` length 2 with the photo first and `primary == true`, `primaryPhoto.url == "/attachments/a-drill-photo"`, `primaryPhoto.thumbnailUrl == "/attachments/a-drill-photo/thumb/500"`, `attachments[1].thumbnailUrl == null` (the manual is `application/pdf`), `fields[0].name == "Voltage"` with `numberValue == 18`, `createdAt` parses as RFC 3339.
- `entity_child_locations_and_items`: Garage → `childLocations` `["Tote A"]`, `items` `["Drill"]`.
- `entity_returns_null_for_unknown_id`.
- `root_items_lists_parentless_items` → `["Loose item"]`.
- `tags_list_with_parent_and_count`: `Electronics.parent.name == "Tools"`, `Tools.entityCount == 1`.
- `search_finds_by_substring_with_limit`: `search(query: "e", limit: 2)` returns 2 rows; `search(query: "drill")` → `["Drill"]`.
- `thumbnail_url_rounds_nothing_server_side_yet`: `primaryPhoto { thumbnailUrl(size: 300) }` → `/attachments/a-drill-photo/thumb/300` (the URL echoes the requested size; the HTTP layer rounds in phase 3).
- In `tests/graphql_summary.rs` add `summary_reports_a_graphql_error_when_currency_is_missing` (phase-1 deferred minor): delete the settings row, POST the summary query, assert status 200, `data.summary` is null and `errors[0].message` contains `currency`.

- [ ] **Step 3: Run to see them fail, implement, run to see them pass**

Run: `cargo test --test graphql_queries --test graphql_summary && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: green. If juniper rejects `Option<&str>` return types, return `Option<String>` with `.clone()`.

- [ ] **Step 4: Commit**

```bash
git add src/graphql tests
git commit -m "feat: full GraphQL Query surface over the inventory

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 6: Homebox importer

**Files:**
- Create: `src/import/mod.rs`, `src/import/source.rs`, `src/import/tables.rs`, `src/import/report.rs`, `src/import/run.rs`, `tests/support/mod.rs`, `tests/import.rs`
- Modify: `Cargo.toml`, `src/lib.rs` (`pub mod import;`)

**Interfaces:**
- Consumes: models, `db::{LOCATION_TYPE_ID, ITEM_TYPE_ID}`, `Cents::from_major_f64`, `AttachmentKind`/`FieldKind`.
- Produces: `import::source::{Source, DirSource, ZipSource, open(path: &Path) -> Result<Box<dyn Source>>}` with `trait Source { fn read_table(&mut self, name: &str) -> Result<Option<Vec<u8>>>; fn read_attachment(&mut self, id: &str) -> Result<Option<Vec<u8>>>; }` (`read_table("manifest")` reads `manifest.json`).
- Produces: `import::tables::*` serde structs (below), `import::report::{ImportReport, TableCounts { inserted: u64, updated: u64, skipped: u64 }}` with `Display` (aligned table + warnings), `import::run::import_backup(conn: &mut SqliteConnection, source: &mut dyn Source, data_dir: &Path) -> Result<ImportReport>`.
- Produces: `tests/support/mod.rs::MiniBackup` fixture builder used by the import tests (`MiniBackup::write_dir(dir: &Path)`, `MiniBackup::write_zip(path: &Path)`), which also encodes a 4×3 JPEG original and a 4×3 WebP thumbnail with the `image` crate.

- [ ] **Step 1: Add dependencies**

In `Cargo.toml` `[dependencies]`:

```toml
hex = "0.4"
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "gif", "webp"] }
sha2 = "0.10"
zip = { version = "8", default-features = false, features = ["deflate"] }
```

Run `cargo build` and then `cargo tree -i openssl; cargo tree -i native-tls` (both empty). Commit `Cargo.lock`.

- [ ] **Step 2: Write `tables.rs`**

```rust
//! The JSON shapes inside a Homebox backup (schemaVersion 1). Column names are
//! Homebox's, including the misleading `entity_children` (it holds the PARENT id).

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: i64,
    #[serde(default)]
    pub exported_at: Option<String>,
    #[serde(default)]
    pub homebox_version: Option<String>,
    #[serde(default)]
    pub counts: HashMap<String, i64>,
}

/// Homebox writes SQLite booleans as 0/1 and Postgres booleans as true/false.
pub fn bool_lenient<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw { B(bool), I(i64), S(String) }
    Ok(match Raw::deserialize(d)? {
        Raw::B(b) => b,
        Raw::I(i) => i != 0,
        Raw::S(s) => matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "t"),
    })
}

/// `"2026-08-27T16:00:08Z"` (any RFC 3339 offset, any precision) → naive UTC.
pub fn parse_timestamp(s: &str) -> anyhow::Result<NaiveDateTime> {
    Ok(DateTime::parse_from_rfc3339(s)?.naive_utc())
}

/// The date part of an RFC 3339 timestamp, or a bare `YYYY-MM-DD`.
pub fn parse_date(s: &str) -> anyhow::Result<NaiveDate> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.date_naive());
    }
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
}

fn empty_as_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|s| !s.is_empty()))
}

#[derive(Debug, Deserialize)]
pub struct EntityTypeRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub icon: Option<String>,
    #[serde(deserialize_with = "bool_lenient")]
    pub is_location: bool,
    #[serde(default)]
    pub entity_type_default_template: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct TagRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub color: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub tag_children: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct EntityRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    pub entity_type_entities: String,
    #[serde(default)]
    pub entity_children: Option<String>,
    #[serde(deserialize_with = "bool_lenient")]
    pub archived: bool,
    #[serde(default)]
    pub asset_id: i64,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub import_ref: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub notes: Option<String>,
    #[serde(default = "one")]
    pub quantity: f64,
    #[serde(deserialize_with = "bool_lenient")]
    pub insured: bool,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub serial_number: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub model_number: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub manufacturer: Option<String>,
    #[serde(deserialize_with = "bool_lenient")]
    pub lifetime_warranty: bool,
    #[serde(default)]
    pub warranty_expires: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub warranty_details: Option<String>,
    #[serde(default)]
    pub purchase_date: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub purchase_from: Option<String>,
    #[serde(default)]
    pub purchase_price: f64,
    #[serde(default)]
    pub sold_date: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub sold_to: Option<String>,
    #[serde(default)]
    pub sold_price: f64,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub sold_notes: Option<String>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub sync_child_entity_locations: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn one() -> f64 { 1.0 }

#[derive(Debug, Deserialize)]
pub struct EntityFieldRow {
    pub id: String,
    pub entity_fields: String, // parent entity id
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text_value: Option<String>,
    #[serde(default)]
    pub number_value: Option<i64>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub boolean_value: bool,
    #[serde(default)]
    pub time_value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct EntityTemplateRow {
    pub id: String,
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub notes: Option<String>,
    #[serde(default = "one")]
    pub default_quantity: f64,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub default_insured: bool,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_name: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_description: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_manufacturer: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_model_number: Option<String>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub default_lifetime_warranty: bool,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub default_warranty_details: Option<String>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub include_warranty_fields: bool,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub include_purchase_fields: bool,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub include_sold_fields: bool,
    #[serde(default)]
    pub default_tag_ids: Option<serde_json::Value>,
    #[serde(default)]
    pub entity_template_location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct TemplateFieldRow {
    pub id: String,
    pub entity_template_fields: String, // parent template id
    pub name: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text_value: Option<String>,
    #[serde(default)]
    pub number_value: Option<i64>,
    #[serde(default, deserialize_with = "bool_lenient")]
    pub boolean_value: bool,
    #[serde(default)]
    pub time_value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentRow {
    pub id: String,
    #[serde(default)]
    pub entity_attachments: Option<String>, // NULL on thumbnail rows
    #[serde(rename = "type")]
    pub kind: String, // includes "thumbnail"
    #[serde(default, deserialize_with = "bool_lenient")]
    pub primary: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub path: String,
    #[serde(default = "octet_stream")]
    pub mime_type: String,
    #[serde(default)]
    pub attachment_thumbnail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn octet_stream() -> String { "application/octet-stream".to_owned() }

#[derive(Debug, Deserialize)]
pub struct TagEntityRow {
    pub tag_id: String,
    pub entity_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn booleans_accept_ints_bools_and_strings() {
        #[derive(Deserialize)]
        struct T { #[serde(deserialize_with = "bool_lenient")] v: bool }
        for (raw, want) in [("1", true), ("0", false), ("true", true), ("false", false), ("\"t\"", true), ("\"0\"", false)] {
            let t: T = serde_json::from_str(&format!("{{\"v\":{raw}}}")).unwrap();
            assert_eq!(t.v, want, "{raw}");
        }
    }

    #[test]
    fn timestamps_and_dates_parse_homebox_shapes() {
        assert_eq!(parse_timestamp("2026-08-27T16:00:08Z").unwrap().to_string(), "2026-08-27 16:00:08");
        assert_eq!(parse_timestamp("2026-09-15T02:47:58.179502537Z").unwrap().to_string(), "2026-09-15 02:47:58.179502537");
        assert_eq!(parse_timestamp("2026-09-15T02:47:58+02:00").unwrap().to_string(), "2026-09-15 00:47:58");
        assert_eq!(parse_date("2021-10-23T00:00:00Z").unwrap().to_string(), "2021-10-23");
        assert_eq!(parse_date("2021-10-23").unwrap().to_string(), "2021-10-23");
        assert!(parse_date("nope").is_err());
    }

    #[test]
    fn an_entity_row_from_the_real_export_shape_deserializes() {
        let raw = r#"{"archived":0,"asset_id":7,"created_at":"2024-09-13T12:23:29.134012559Z","description":"","entity_children":null,"entity_type_entities":"t1","group_entities":"g","id":"e1","import_ref":null,"insured":0,"lifetime_warranty":0,"manufacturer":null,"model_number":null,"name":"Attic","notes":null,"purchase_date":null,"purchase_from":null,"purchase_price":0,"quantity":1,"serial_number":null,"sold_date":null,"sold_notes":null,"sold_price":0,"sold_to":null,"sync_child_entity_locations":0,"updated_at":"2026-08-27T16:00:11.501644955Z","warranty_details":null,"warranty_expires":null}"#;
        let row: EntityRow = serde_json::from_str(raw).unwrap();
        assert_eq!(row.asset_id, 7);
        assert_eq!(row.description, None);
        assert_eq!(row.entity_children, None);
        assert!(!row.archived);
    }
}
```

Unknown extra keys (`group_entities`, `group_tags`, …) are ignored by serde's default.

- [ ] **Step 3: Write `source.rs`**

```rust
//! Where a backup's bytes come from: an exploded directory or the zip itself.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

pub trait Source {
    /// `read_table("entities")` returns the bytes of `entities.json`, or `None` if absent.
    fn read_table(&mut self, name: &str) -> Result<Option<Vec<u8>>>;
    /// The raw blob stored under `attachments/<id>`, or `None` if absent.
    fn read_attachment(&mut self, id: &str) -> Result<Option<Vec<u8>>>;
    fn describe(&self) -> String;
}

pub struct DirSource { root: PathBuf }

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: root.into() } }
    fn read_optional(&self, rel: &Path) -> Result<Option<Vec<u8>>> {
        let path = self.root.join(rel);
        match fs::read(&path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }
}

impl Source for DirSource {
    fn read_table(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        self.read_optional(Path::new(&format!("{name}.json")))
    }
    fn read_attachment(&mut self, id: &str) -> Result<Option<Vec<u8>>> {
        self.read_optional(&Path::new("attachments").join(id))
    }
    fn describe(&self) -> String { format!("directory {}", self.root.display()) }
}

pub struct ZipSource { archive: ZipArchive<File>, path: PathBuf }

impl ZipSource {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = File::open(&path).with_context(|| format!("opening {}", path.display()))?;
        let archive = ZipArchive::new(file).with_context(|| format!("{} is not a zip file", path.display()))?;
        Ok(Self { archive, path })
    }
    fn read_entry(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        let mut entry = match self.archive.by_name(name) {
            Ok(e) => e,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading {name} from the zip")),
        };
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(Some(buf))
    }
}

impl Source for ZipSource {
    fn read_table(&mut self, name: &str) -> Result<Option<Vec<u8>>> { self.read_entry(&format!("{name}.json")) }
    fn read_attachment(&mut self, id: &str) -> Result<Option<Vec<u8>>> { self.read_entry(&format!("attachments/{id}")) }
    fn describe(&self) -> String { format!("zip {}", self.path.display()) }
}

/// A `.zip` file becomes a `ZipSource`; a directory becomes a `DirSource`.
pub fn open(path: &Path) -> Result<Box<dyn Source>> {
    if path.is_dir() {
        Ok(Box::new(DirSource::new(path)))
    } else if path.is_file() {
        Ok(Box::new(ZipSource::open(path)?))
    } else {
        bail!("{} is neither a directory nor a file", path.display())
    }
}
```

- [ ] **Step 4: Write `report.rs`**

```rust
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TableCounts { pub inserted: u64, pub updated: u64, pub skipped: u64 }

#[derive(Debug, Default)]
pub struct ImportReport {
    pub tables: BTreeMap<&'static str, TableCounts>,
    pub warnings: Vec<String>,
    pub originals_written: u64,
}

impl ImportReport {
    pub fn counts(&mut self, table: &'static str) -> &mut TableCounts { self.tables.entry(table).or_default() }
    pub fn warn(&mut self, message: impl Into<String>) {
        let m = message.into();
        tracing::warn!("{m}");
        self.warnings.push(m);
    }
}

impl fmt::Display for ImportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:<20} {:>9} {:>9} {:>9}", "table", "inserted", "updated", "skipped")?;
        for (name, c) in &self.tables {
            writeln!(f, "{name:<20} {:>9} {:>9} {:>9}", c.inserted, c.updated, c.skipped)?;
        }
        writeln!(f, "originals written: {}", self.originals_written)?;
        if self.warnings.is_empty() {
            writeln!(f, "warnings: none")
        } else {
            writeln!(f, "warnings ({}):", self.warnings.len())?;
            for w in &self.warnings { writeln!(f, "  - {w}")?; }
            Ok(())
        }
    }
}
```

- [ ] **Step 5: Write the fixture builder and the failing import tests**

`tests/support/mod.rs` builds a "mini" backup that mirrors the real one's shapes: three types (`global.location`, `global.item`, `Tote` with a Homebox uuid), entities `Garage` (location, root, asset 2), `Tote 1` (Tote, parent Garage), `Router` (item, parent Garage, price `1099.99`, qty 1, asset 7, purchase_date `2021-10-23T00:00:00Z`, insured 1), `Cable` (item, parent Tote 1, price `4.5`, qty 3), tags `IOT` and `General` (General's `tag_children` = IOT), `tag_entities` Router→IOT, one custom field on Router (`type: "text"`, `text_value: "AX1800"`), attachments: photo row for Router (`attachment_thumbnail` → thumb id, `path` ends with the real sha256 of the JPEG bytes, `primary: 1`, `mime_type: image/jpeg`, `title: image.jpg`) and its thumbnail row (`type: thumbnail`, `entity_attachments: null`, `mime_type: image/webp`), plus a `manual` attachment on Router (`application/pdf`, bytes `%PDF-1.4 mini`), plus `maintenance_entries.json` with one row and `notifiers.json` with `[]`. The JPEG is a 4×3 `image::RgbImage` encoded with `image::codecs::jpeg::JpegEncoder`; the WebP is the same image encoded with `image::codecs::webp::WebPEncoder::new_lossless`. `manifest.json` has `schemaVersion: 1`, `counts` matching. `write_dir` writes all of it; `write_zip` writes the same entries into a zip with `zip::write::SimpleFileOptions::default()` (deflate). The builder returns a `MiniIds { garage, tote1, router, cable, iot, general, photo, thumb, manual, tote_type, jpeg_sha256 }`.

`tests/import.rs` cases (each `#[test]`, sync, using `TestDb` and a `tempfile::tempdir()` as `data_dir`):

1. `imports_the_mini_backup_from_a_directory`: counts: `entity_types` inserted 1 (Tote) + updated 0 + skipped 2 (built-ins mapped by name are reported as `skipped`), `entities` inserted 4, `tags` inserted 2, `tag_entities` 1, `entity_fields` 1, `attachments` inserted 2, `thumbnails` inserted 1; `maintenance_entries` warning mentions `1`; `report.warnings` has exactly that one warning.
2. `maps_built_in_types_and_keeps_other_ids`: Garage's `entity_type_id == LOCATION_TYPE_ID`, Router's `== ITEM_TYPE_ID`, Tote 1's `== ids.tote_type`.
3. `parent_links_use_entity_children`: Tote 1's `parent_id == Some(garage)`, Cable's `== Some(tote1)`, Garage's `None`.
4. `money_dates_and_flags_convert`: Router `purchase_price_cents == Cents(109_999)`, `purchase_date == 2021-10-23`, `insured`, `asset_id == AssetId(7)`; Cable `Cents(450)`, `quantity == 3.0`.
5. `tags_and_custom_fields_link`: `svc::tag::for_entity(router)` → [IOT]; General's `parent_id == Some(iot)`; Router field `text_value == Some("AX1800")`, `kind == FieldKind::Text`.
6. `originals_are_content_addressed_and_thumbnails_stored_at_500`: file `data_dir/originals/<jpeg_sha256>` exists with the JPEG bytes; the photo row has `sha256 == jpeg_sha256`, `size_bytes` = length, `kind == Photo`, `is_primary`; the manual row has `kind == Manual`; `svc::attachment::thumbnail(photo, 500)` is `Some` with `width 4, height 3, mime image/webp` and `data` equal to the WebP bytes; `report.originals_written == 2`.
7. `rerunning_is_idempotent_and_never_deletes`: run twice; second report has inserted 0 everywhere and updated 4 entities; row counts unchanged; `originals_written == 0` the second time.
8. `zip_and_directory_sources_agree`: import from the zip into a second db; `entities`/`attachments` row sets equal by id.
9. `refuses_other_schema_versions`: manifest `schemaVersion: 2` → `Err` whose message contains `schemaVersion 2`.
10. `missing_optional_tables_are_fine`: delete `entity_fields.json` and `entity_templates.json` from the dir → import succeeds, zero counts for them.
11. `an_unknown_parent_becomes_a_root_with_a_warning` (Review Focus 1): edit `entities.json` so Cable's `entity_children` is `"missing-id"`; Cable imports with `parent_id None`; a warning names `Cable`'s id and `missing-id`.
12. `an_attachment_without_a_blob_is_skipped_with_a_warning` (Review Focus 2): delete `attachments/<manual id>`; the manual row is absent, `attachments.skipped == 1`, a warning names the id; the photo still imports.
13. `a_path_hash_mismatch_is_a_warning_not_an_error`: set the photo row's `path` to end with `deadbeef`; import succeeds, the stored `sha256` is the real hash, a warning mentions the id.

- [ ] **Step 6: Write `run.rs`**

```rust
//! Upserts a Homebox backup in dependency order inside one transaction.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use diesel::prelude::*;
use sha2::{Digest, Sha256};

use super::report::ImportReport;
use super::source::Source;
use super::tables::*;
use crate::db::{ITEM_TYPE_ID, LOCATION_TYPE_ID};
use crate::kinds::{AttachmentKind, FieldKind};
use crate::money::Cents;
use crate::asset_id::AssetId;
use crate::schema::*;

const SUPPORTED_SCHEMA_VERSION: i64 = 1;
const HOMEBOX_THUMBNAIL_SIZE: i32 = 500;

pub fn import_backup(conn: &mut SqliteConnection, source: &mut dyn Source, data_dir: &Path) -> Result<ImportReport> {
    let manifest: Manifest = read_table(source, "manifest")?
        .context("manifest.json is missing; is this a Homebox backup?")?;
    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        bail!(
            "unsupported backup: schemaVersion {} (this importer understands schemaVersion {SUPPORTED_SCHEMA_VERSION})",
            manifest.schema_version
        );
    }

    // Read everything first so a malformed file fails before any write.
    let types: Vec<EntityTypeRow> = read_table(source, "entity_types")?.unwrap_or_default();
    let templates: Vec<EntityTemplateRow> = read_table(source, "entity_templates")?.unwrap_or_default();
    let template_fields: Vec<TemplateFieldRow> = read_table(source, "template_fields")?.unwrap_or_default();
    let tag_rows: Vec<TagRow> = read_table(source, "tags")?.unwrap_or_default();
    let entity_rows: Vec<EntityRow> = read_table(source, "entities")?.unwrap_or_default();
    let field_rows: Vec<EntityFieldRow> = read_table(source, "entity_fields")?.unwrap_or_default();
    let tag_links: Vec<TagEntityRow> = read_table(source, "tag_entities")?.unwrap_or_default();
    let attachment_rows: Vec<AttachmentRow> = read_table(source, "attachments")?.unwrap_or_default();
    let maintenance: Vec<serde_json::Value> = read_table(source, "maintenance_entries")?.unwrap_or_default();
    let notifiers: Vec<serde_json::Value> = read_table(source, "notifiers")?.unwrap_or_default();

    let originals_dir = data_dir.join("originals");
    fs::create_dir_all(&originals_dir).with_context(|| format!("creating {}", originals_dir.display()))?;

    // Blobs are read (and hashed) outside the transaction; the copy is idempotent.
    let mut report = ImportReport::default();
    if !maintenance.is_empty() {
        report.warn(format!("{} maintenance entries were not imported (not supported in v1)", maintenance.len()));
    }
    if !notifiers.is_empty() {
        report.warn(format!("{} notifiers were not imported (not supported in v1)", notifiers.len()));
    }

    conn.transaction::<_, anyhow::Error, _>(|conn| {
        let type_ids = import_entity_types(conn, &types, &mut report)?;
        import_templates(conn, &templates, &template_fields, &mut report)?;
        import_tags(conn, &tag_rows, &mut report)?;
        let entity_ids = import_entities(conn, &entity_rows, &type_ids, &mut report)?;
        patch_default_templates(conn, &types, &type_ids, &templates)?;
        import_entity_fields(conn, &field_rows, &entity_ids, &mut report)?;
        import_tag_links(conn, &tag_links, &entity_ids, &mut report)?;
        import_attachments(conn, source, &attachment_rows, &entity_ids, &originals_dir, &mut report)?;
        Ok(())
    })?;
    Ok(report)
}

fn read_table<T: serde::de::DeserializeOwned>(source: &mut dyn Source, name: &str) -> Result<Option<T>> {
    match source.read_table(name)? {
        None => Ok(None),
        Some(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .with_context(|| format!("parsing {name}.json")),
    }
}

/// Returns Homebox type id → our type id (built-ins remapped by name).
fn import_entity_types(conn: &mut SqliteConnection, rows: &[EntityTypeRow], report: &mut ImportReport) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for row in rows {
        let ours = match row.name.as_str() {
            "global.location" => Some(LOCATION_TYPE_ID),
            "global.item" => Some(ITEM_TYPE_ID),
            _ => None,
        };
        if let Some(builtin) = ours {
            map.insert(row.id.clone(), builtin.to_owned());
            report.counts("entity_types").skipped += 1;
            continue;
        }
        let existed = exists(conn, entity_types::table.filter(entity_types::id.eq(&row.id)))?;
        diesel::insert_into(entity_types::table)
            .values((
                entity_types::id.eq(&row.id),
                entity_types::name.eq(&row.name),
                entity_types::description.eq(&row.description),
                entity_types::icon.eq(&row.icon),
                entity_types::is_location.eq(row.is_location),
                entity_types::created_at.eq(parse_timestamp(&row.created_at)?),
                entity_types::updated_at.eq(parse_timestamp(&row.updated_at)?),
            ))
            .on_conflict(entity_types::id)
            .do_update()
            .set((
                entity_types::name.eq(&row.name),
                entity_types::description.eq(&row.description),
                entity_types::icon.eq(&row.icon),
                entity_types::is_location.eq(row.is_location),
                entity_types::updated_at.eq(parse_timestamp(&row.updated_at)?),
            ))
            .execute(conn)?;
        bump(report.counts("entity_types"), existed);
        map.insert(row.id.clone(), row.id.clone());
    }
    Ok(map)
}
```

The remaining functions follow the same upsert shape (`insert … on_conflict(pk).do_update().set(all non-key columns)` plus `exists` before to count inserted vs updated):

- `import_templates`: templates first (with `location_id = NULL` in pass one), then `template_fields` (`template_id = entity_template_fields`), then a second pass that sets `location_id` from `entity_template_location` for templates whose target entity exists (after entities are imported; so call this second pass from inside `patch_default_templates` or split into `import_templates` + `patch_template_locations` called after `import_entities`; the implementer chooses the split but both patches must run after entities exist). `default_tag_ids` is stored as its JSON text (`serde_json::to_string`).
- `import_tags`: upsert all with `parent_id = NULL`, then a second loop `UPDATE tags SET parent_id = ? WHERE id = ?` for rows whose `tag_children` names an imported tag; unknown targets produce a warning.
- `import_entities`: upsert all with `parent_id = NULL`, mapping `entity_type_entities` through `type_ids` (an unknown type id is an error: `bail!("entity {} references unknown entity type {}", …)`), converting money/dates/asset id, then a second loop setting `parent_id` where `entity_children` is `Some(p)` and `p` is in the imported set; otherwise `report.warn(format!("entity {} references missing parent {}; imported as a root", row.id, p))` (Review Focus 1). Returns the `HashSet<String>` of imported entity ids.
- `patch_default_templates`: for each type row with `entity_type_default_template = Some(t)` where `t` was imported, `UPDATE entity_types SET default_template_id`.
- `import_entity_fields`: `kind` parsed with `FieldKind::from_str` (an unknown kind is a warning + skip), `entity_id = entity_fields`, skip with warning if the entity was not imported, `time_value` via `parse_timestamp` when present.
- `import_tag_links`: `INSERT OR IGNORE` (`.on_conflict_do_nothing()`); count inserted by comparing `execute` result (1 inserted, 0 ignored → `skipped`); skip with a warning when the entity or tag is unknown.
- `import_attachments`: first index thumbnail rows by id (`kind == "thumbnail"`), then for every non-thumbnail row: `entity_attachments` missing/unknown → warning + skip; `AttachmentKind::from_str(kind)` unknown → warning + skip; `source.read_attachment(&row.id)?` `None` → `report.warn(format!("attachment {} has no blob in the backup; skipped", row.id)); skipped += 1; continue` (Review Focus 2); compute `sha256 = hex::encode(Sha256::digest(&bytes))`; if `row.path` does not end with `&sha256` → warning (path hash mismatch, ours wins); write `originals_dir/<sha256>` only if it does not exist (`originals_written += 1` when written); upsert the attachment row (`kind`, `is_primary = row.primary`, `title`, `mime_type`, `sha256`, `size_bytes = bytes.len()`); then if `row.attachment_thumbnail` names a thumbnail row with a readable blob: decode dimensions with `image::load_from_memory(&thumb_bytes)` (on decode failure: warning, no thumbnail), upsert into `thumbnails` at `(attachment_id, 500)` with `mime_type = thumb_row.mime_type`, `width`, `height`, `data = thumb_bytes`; count under `"thumbnails"`.

Helpers:

```rust
fn exists<Q>(conn: &mut SqliteConnection, filter: Q) -> Result<bool>
where Q: diesel::query_dsl::methods::SelectDsl<diesel::dsl::CountStar> /* or simply take a closure */ 
```

Simpler and what the plan mandates: write `fn exists(conn, table: &'static str, id: &str) -> Result<bool>` using `diesel::sql_query(format!("SELECT count(*) AS n FROM {table} WHERE id = ?")).bind::<Text, _>(id)` with the `Count` QueryableByName pattern from `db.rs` tests (table names are compile-time literals from this module, never user input). And `fn bump(c: &mut TableCounts, existed: bool) { if existed { c.updated += 1 } else { c.inserted += 1 } }`.

- [ ] **Step 7: Run the import tests, then everything**

Run: `cargo test --test import && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`
Expected: 13 import tests pass, nothing else regressed.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/import tests/support tests/import.rs
git commit -m "feat: Homebox backup importer with directory and zip sources

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 7: `import` subcommand and the `serve` message fix

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `import::source::open`, `import::run::import_backup`, `db::{build_pool, run_migrations}`, `Config`.

- [ ] **Step 1: Wire the command**

Replace the `Command::Import` arm with `Command::Import { path } => import(config, path)`, and add:

```rust
fn import(config: Config, path: PathBuf) -> Result<()> {
    use anyhow::Context as _;
    use home_tracker::{db, import};

    let pool = db::build_pool(&config.database_url)?;
    let mut conn = pool.get().context("connection for import")?;
    db::run_migrations(&mut conn)?;
    let mut source = import::source::open(&path)?;
    tracing::info!(source = %source.describe(), data_dir = %config.data_dir.display(), "importing Homebox backup");
    let report = import::run::import_backup(&mut conn, source.as_mut(), &config.data_dir)?;
    println!("{report}");
    Ok(())
}
```

Also fix the `serve` error context (phase-1 deferred minor): the `create_dir_all(config.data_dir.join("originals"))` context must name the `originals` path it creates.

- [ ] **Step 2: Manual check with the fixture and commit**

Run `cargo run -- import tests/fixtures-does-not-exist` → error `is neither a directory nor a file`, exit 1. Then, with a scratch env (`DATABASE_URL=$SCRATCH/db.sqlite DATA_DIR=$SCRATCH`), import the mini backup written by a tiny one-off test or by running `cargo test --test import -- --nocapture imports_the_mini_backup_from_a_directory` after temporarily printing the temp path — simplest: add a `#[test] #[ignore] fn write_mini_backup_to_target_dir()` in `tests/import.rs` that writes the fixture to `/home/.build/cargo-target/mini-backup` (the shared cargo target dir), run it with `cargo test --test import -- --ignored write_mini_backup_to_target_dir`, then `cargo run -- import /home/.build/cargo-target/mini-backup` and paste the printed report into the task report. Remove the scratch db afterwards.

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all --check`

```bash
git add src/main.rs tests/import.rs
git commit -m "feat: import subcommand runs the Homebox importer

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

### Task 8: Real-backup acceptance run and docs

**Files:**
- Modify: `README.md`, `CLAUDE.md`

- [ ] **Step 1: Import Steve's real backup into a scratch database**

```bash
S=/home/.build/cargo-target/phase2-acceptance && rm -rf "$S" && mkdir -p "$S"
DATABASE_URL=$S/db.sqlite DATA_DIR=$S cargo run --release -- import homebox-backup
```

Expected: exit 0; report shows `entity_types` inserted 2 / skipped 2, `entities` inserted 86, `tags` 6, `tag_entities` 66, `attachments` inserted 38, `thumbnails` inserted 38, `originals written: 38` (there are 38 photo rows plus 38 thumbnail rows in the 76), `warnings: none`. Then:

```bash
PORT=7021 DATABASE_URL=$S/db.sqlite DATA_DIR=$S cargo run --release &
sleep 2
curl -s -X POST localhost:7021/graphql -H 'content-type: application/json' \
  -d '{"query":"{ summary { totalValueCents currency totalItems totalLocations totalTags } locationTree { entity { name } children { entity { name } } } }"}'
kill %1
```

Expected: `"summary":{"totalValueCents":293294,"currency":"USD","totalItems":67,"totalLocations":19,"totalTags":6}` and a tree whose roots include `Attic`, `Garage` and the other top-level locations from the backup. Record the full output in the task report. Also run `ls "$S/originals" | wc -l` → 38, and `find "$S/originals" -type f -size 0` → nothing. Then delete `$S`. If any number differs, stop and report DONE_WITH_CONCERNS with the diff; do not adjust the formulas.

- [ ] **Step 2: Update the docs**

README: the "Importing a Homebox backup" section becomes real (`home-tracker import backup.zip` or an exploded directory; what it does; re-running is safe; where originals go; what is not imported: maintenance entries, notifiers). Project layout gains `src/import/`, `src/models/`, `src/asset_id.rs`, `src/money.rs`, `src/kinds.rs`. CLAUDE.md: Architecture lists the new modules; Key Conventions adds "importer keeps Homebox UUIDs and maps built-in types by name onto the seeded ids; enum columns are lowercase text behind `kinds.rs`; every `svc` read returns `Selectable` models via `as_select()`". Keep CLAUDE.md under 120 lines. No em dashes.

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: describe the importer and the inventory modules

Claude-Session: https://claude.ai/code/session_01DX5JMWA5ti2dhpD4zuDkbF"
```

---

## Self-review

- **Spec coverage (Phase 2 exit criteria, spec §15):** full schema (T1), models (T3), `svc` reads (T3), the whole Query side of §6 (T5), real `summary` (T4), value types (T2), importer with fixture (T6), `import` CLI (T7), real backup imports with matching numbers (T8), every Query field has an integration test (T5). `just import` already exists from phase 1.
- **Placeholders:** none. Where a function body is described rather than pasted (the sibling upsert functions in T6, the sibling objects in T5, the sibling models in T3), the pasted example fixes the exact shape and the description names every column and rule.
- **Type consistency:** `Cents::from_major_f64`/`as_graphql_int`, `AssetId::display_option`, `AttachmentKind`/`FieldKind::from_str`, `svc::fixtures::seed_sample -> SampleIds`, `svc::entity::{get, is_location, children, child_locations, items, root_items, ancestors, location_tree, search}`, `svc::attachment::{for_entity, primary_photo, get, thumbnail}`, `svc::tag::{list, get, for_entity, entity_count}`, `import::source::{Source, open}`, `import::run::import_backup`, `ImportReport::{counts, warn}`, `TableCounts` are spelled identically in every task that uses them.
- **Review Focus:** 1 → T6 test 11; 2 → T6 test 12; 3 → T3 `ancestors_terminate_on_a_cycle` and `location_tree_treats_an_orphaned_location_as_a_root`; 4 → T4 `an_empty_database_reports_zeros`; 5 → T3 `search_matches_name_case_insensitively_and_escapes_wildcards`.
- **Phase-1 carry-overs closed here:** PRAGMA-value tests and private `MIGRATIONS` (T1), `serve` error message (T7), `/graphql`-seam missing-currency test (T5).
