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
