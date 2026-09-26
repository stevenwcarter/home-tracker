-- Back to one row per attachment: each attachment sharing a sha gets its own
-- copy of that sha's thumbnails. Rows no attachment references are dropped.
CREATE TABLE thumbnails_old (
  attachment_id TEXT NOT NULL REFERENCES attachments(id) ON DELETE CASCADE,
  size INTEGER NOT NULL,
  mime_type TEXT NOT NULL,
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  data BLOB NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (attachment_id, size)
);
INSERT INTO thumbnails_old (attachment_id, size, mime_type, width, height, data, created_at)
  SELECT a.id, t.size, t.mime_type, t.width, t.height, t.data, t.created_at
  FROM thumbnails t JOIN attachments a ON a.sha256 = t.sha256;
DROP TABLE thumbnails;
ALTER TABLE thumbnails_old RENAME TO thumbnails;
