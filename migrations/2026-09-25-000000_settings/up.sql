CREATE TABLE settings (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);

INSERT INTO settings (key, value) VALUES ('currency', 'USD');
