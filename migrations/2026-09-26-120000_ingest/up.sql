-- AI ingest staging: a batch of items, each a group of photos waiting to be
-- described, reviewed and accepted as an entity. Photos share the originals
-- store (and the sha-keyed thumbnails) with attachments.
CREATE TABLE ingest_batches (
  id TEXT PRIMARY KEY NOT NULL,
  parent_id TEXT REFERENCES entities(id) ON DELETE SET NULL,
  status TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE ingest_items (
  id TEXT PRIMARY KEY NOT NULL,
  batch_id TEXT NOT NULL REFERENCES ingest_batches(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  status TEXT NOT NULL,
  error TEXT,
  suggestion TEXT,
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
  status TEXT NOT NULL,
  error TEXT,
  description TEXT,
  suggested_kind TEXT,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX ingest_items_batch ON ingest_items(batch_id, position);
CREATE INDEX ingest_photos_item ON ingest_photos(item_id, position);
CREATE INDEX ingest_photos_sha ON ingest_photos(sha256);
