-- Thumbnails are keyed by the content hash of their original instead of by
-- attachment, so attachments (and later ingest photos) that share bytes share
-- one set of thumbnails. Rows of attachments sharing a sha collapse into one.
CREATE TABLE thumbnails_new (
  sha256 TEXT NOT NULL,
  size INTEGER NOT NULL,
  mime_type TEXT NOT NULL,
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  data BLOB NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (sha256, size)
);
INSERT OR IGNORE INTO thumbnails_new (sha256, size, mime_type, width, height, data, created_at)
  SELECT a.sha256, t.size, t.mime_type, t.width, t.height, t.data, t.created_at
  FROM thumbnails t JOIN attachments a ON a.id = t.attachment_id;
DROP TABLE thumbnails;
ALTER TABLE thumbnails_new RENAME TO thumbnails;
