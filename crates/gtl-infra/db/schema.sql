CREATE VIEW active_projects AS
SELECT
    projects.[id],
    projects.[title],
    project_sources.[source_kind],
    project_sources.[source_value],
    projects.[git_remote],
    projects.[mux_session_name],
    projects.[affiliation],
    projects.[color],
    projects.[created_at]
FROM projects
JOIN project_sources USING ([source_id])
WHERE projects.[paused_at] IS NULL
  AND projects.[unmanaged_at] IS NULL;

CREATE TABLE "live_views" (
  id INTEGER PRIMARY KEY,
  source_kind TEXT NOT NULL,
  source_value TEXT NOT NULL,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  last_opened_at TEXT,
  comparison TEXT NOT NULL DEFAULT 'unpushed_commits' CHECK (comparison IN ('local_changes', 'unpushed_commits')),
  UNIQUE (source_kind, source_value, comparison)
) STRICT;

CREATE TABLE pinned_viewer_tabs (
    position INTEGER PRIMARY KEY,
    recipe_json TEXT NOT NULL CHECK (json_valid(recipe_json)),
    live INTEGER NOT NULL CHECK (live IN (0, 1))
) STRICT;

CREATE TABLE project_groups (
    [project_id] TEXT NOT NULL
        REFERENCES projects ([id]) ON DELETE CASCADE,
    [group_name] TEXT NOT NULL
        CHECK ([group_name] = trim([group_name]) AND length([group_name]) > 0),
    PRIMARY KEY ([project_id], [group_name])
) STRICT;

CREATE TABLE project_render_recency (
  source_value TEXT PRIMARY KEY,
  rendered_at TEXT NOT NULL
) STRICT;

CREATE TABLE project_sources (
    [source_id] INTEGER PRIMARY KEY,
    [source_kind] TEXT NOT NULL,
    [source_value] TEXT NOT NULL,
    UNIQUE ([source_kind], [source_value]),
    CHECK ([source_kind] = 'directory')
) STRICT;

CREATE TABLE project_status_index (
  project_id TEXT PRIMARY KEY REFERENCES projects (id) ON DELETE CASCADE,
  source_id INTEGER NOT NULL REFERENCES project_sources (source_id),
  comparison_branch TEXT NOT NULL,
  branch TEXT,
  commits_ahead INTEGER CHECK (commits_ahead >= 0),
  tracked_changes INTEGER CHECK (tracked_changes IN (0, 1)),
  untracked_changes INTEGER CHECK (untracked_changes IN (0, 1)),
  checked_at INTEGER NOT NULL
) STRICT;

CREATE TABLE projects (
    [id] TEXT PRIMARY KEY
        CHECK (length([id]) BETWEEN 2 AND 4 AND [id] NOT GLOB '*[^A-Z]*'),
    [source_id] INTEGER NOT NULL UNIQUE
        REFERENCES project_sources ([source_id]),
    [title] TEXT NOT NULL COLLATE NOCASE UNIQUE
        CHECK ([title] = trim([title]) AND length([title]) > 0),
    [git_remote] TEXT
        CHECK ([git_remote] IS NULL OR ([git_remote] = trim([git_remote]) AND length([git_remote]) > 0)),
    [mux_session_name] TEXT NOT NULL UNIQUE
        CHECK (
            length([mux_session_name]) BETWEEN 1 AND 64
        ),
    [affiliation] TEXT NOT NULL
        CHECK ([affiliation] IN ('personal', 'work')),
    [color] TEXT
        CHECK (
            [color] IS NULL
            OR (
                length([color]) = 7
                AND substr([color], 1, 1) = '#'
                AND substr([color], 2) NOT GLOB '*[^0-9a-f]*'
            )
        ),
    [export_include_in_all] INTEGER
        CHECK (
            [export_include_in_all] IS NULL
            OR [export_include_in_all] IN (0, 1)
        ),
    [created_at] TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        CHECK (strftime('%Y-%m-%dT%H:%M:%fZ', [created_at]) IS [created_at]),
    [paused_at] TEXT
        CHECK (strftime('%Y-%m-%dT%H:%M:%fZ', [paused_at]) IS [paused_at]),
    [unmanaged_at] TEXT
        CHECK (strftime('%Y-%m-%dT%H:%M:%fZ', [unmanaged_at]) IS [unmanaged_at])
, comparison_branch TEXT NOT NULL DEFAULT 'main'
    CHECK (length(comparison_branch) BETWEEN 1 AND 1024 AND comparison_branch = trim(comparison_branch))) STRICT;

CREATE TABLE "recent_renders" (
  id           INTEGER PRIMARY KEY,
  source_id    INTEGER NOT NULL REFERENCES "render_sources" (id),
  operation_id INTEGER NOT NULL REFERENCES render_operations (id),
  target_id    INTEGER REFERENCES render_targets (id),
  argument     TEXT,
  pinned_base  TEXT,
  pinned_head  TEXT,
  recipe_name  TEXT,
  title        TEXT NOT NULL,
  repo_name    TEXT NOT NULL,
  range_label  TEXT NOT NULL,
  rendered_at  TEXT NOT NULL, project_id TEXT REFERENCES projects (id),
  CHECK ((pinned_base IS NULL) = (pinned_head IS NULL)),
  -- Only the diff operation (seeded id 1) takes a target.
  CHECK ((operation_id = 1) = (target_id IS NOT NULL))
) STRICT;

CREATE UNIQUE INDEX recent_renders_fingerprint_idx
ON recent_renders (
  source_id,
  repo_name,
  coalesce(pinned_base, X''),
  coalesce(pinned_head, X'')
);

CREATE INDEX recent_renders_project_id_idx ON recent_renders (project_id, id DESC);

CREATE INDEX recent_renders_repo_name_idx
ON recent_renders (repo_name);

CREATE TABLE render_operations (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE "render_sources" (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  kind       TEXT NOT NULL CHECK (kind IN ('directory', 'remote')),
  value      TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT,
  UNIQUE (kind, value)
) STRICT;

CREATE INDEX render_sources_value_idx ON render_sources (value);

CREATE TABLE render_targets (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
) STRICT;
