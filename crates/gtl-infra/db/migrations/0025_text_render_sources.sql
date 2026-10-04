-- History records diff text by its identity. SQLite cannot change a CHECK constraint, so the
-- source table is rebuilt; recent_renders keeps referencing the same ids, which are restored
-- before the deferred foreign keys are checked at commit.
PRAGMA defer_foreign_keys = ON;

CREATE TEMP TABLE render_sources_migration_backup AS
SELECT id, kind, value, created_at, updated_at FROM render_sources;

DROP TABLE render_sources;

CREATE TABLE "render_sources" (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  kind       TEXT NOT NULL CHECK (kind IN ('directory', 'remote', 'text')),
  value      TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT,
  UNIQUE (kind, value)
) STRICT;

INSERT INTO render_sources (id, kind, value, created_at, updated_at)
SELECT id, kind, value, created_at, updated_at FROM render_sources_migration_backup;

DROP TABLE render_sources_migration_backup;

CREATE INDEX render_sources_value_idx ON render_sources (value);

INSERT INTO render_operations (id, name) VALUES (3, 'text');
