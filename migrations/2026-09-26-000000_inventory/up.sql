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
