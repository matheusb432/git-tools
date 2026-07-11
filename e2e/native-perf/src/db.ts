// Seeds `live_views` rows directly into the isolated `<GIT_TOOLS_DATA_DIR>/gtl.db` before the
// viewer process ever opens it, so restore-on-startup scenarios (a healthy live view, a broken
// one) can be asserted at session start without driving a mid-session app restart. The schema
// mirrors schema v1 in `crates/infra/src/app_state/db.rs`; `user_version` is set to that
// migration's target version so the app's own `rusqlite_migration` run sees the schema as
// already applied (`Ordering::Equal` short-circuits to a no-op) instead of re-running
// `CREATE TABLE` against tables this script already created.
import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { join } from "node:path";

const SCHEMA_V1 = `
CREATE TABLE live_views (
  id             INTEGER PRIMARY KEY,
  source_kind    TEXT NOT NULL,
  source_value   TEXT NOT NULL,
  display_name   TEXT NOT NULL,
  created_at     TEXT NOT NULL,
  last_opened_at TEXT,
  UNIQUE (source_kind, source_value)
) STRICT;

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;

CREATE TABLE recent_renders (
  id          INTEGER PRIMARY KEY,
  recipe_json TEXT NOT NULL,
  title       TEXT NOT NULL,
  repo_name   TEXT NOT NULL,
  kind        TEXT NOT NULL,
  range_label TEXT NOT NULL,
  rendered_at TEXT NOT NULL
) STRICT;
`;

/** The db version schema v1's migration leaves `PRAGMA user_version` at. */
const SCHEMA_V1_USER_VERSION = 1;

export type LiveViewSeed = {
  readonly sourceKind: string;
  readonly sourceValue: string;
  readonly displayName: string;
  readonly createdAt?: string;
  readonly lastOpenedAt?: string | null;
};

export type ViewerSettingsSeed = Readonly<Record<string, string>>;

/** Writes `live_views` rows into `<dataDir>/gtl.db`, creating schema v1 fresh. Must run before
 * the viewer process first opens the db for this data dir — after startup, drive persistence
 * through the app's own `save_live_view` command instead. */
export function seedLiveViews(dataDir: string, rows: readonly LiveViewSeed[]): void {
  mkdirSync(dataDir, { recursive: true });
  const db = new Database(join(dataDir, "gtl.db"), { create: true });
  try {
    db.exec(SCHEMA_V1);
    db.exec(`PRAGMA user_version = ${SCHEMA_V1_USER_VERSION};`);

    const insert = db.prepare(
      `INSERT INTO live_views (source_kind, source_value, display_name, created_at, last_opened_at)
       VALUES (?, ?, ?, ?, ?)
       ON CONFLICT(source_kind, source_value) DO UPDATE SET display_name = excluded.display_name`,
    );
    const createdAtDefault = new Date().toISOString();
    for (const row of rows) {
      insert.run(
        row.sourceKind,
        row.sourceValue,
        row.displayName,
        row.createdAt ?? createdAtDefault,
        row.lastOpenedAt ?? null,
      );
    }
  } finally {
    db.close();
  }
}

/** Writes persisted viewer settings into the already-seeded app-state database. */
export function seedViewerSettings(dataDir: string, settings: ViewerSettingsSeed): void {
  const db = new Database(join(dataDir, "gtl.db"));
  try {
    const insert = db.prepare(
      `INSERT INTO settings (key, value) VALUES (?, ?)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
    );
    for (const [key, value] of Object.entries(settings)) {
      insert.run(key, value);
    }
  } finally {
    db.close();
  }
}
