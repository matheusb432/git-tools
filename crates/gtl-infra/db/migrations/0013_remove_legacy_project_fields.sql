PRAGMA defer_foreign_keys = ON;

DROP VIEW active_projects;

CREATE TEMP TABLE project_groups_migration_backup AS
SELECT project_id, group_name FROM project_groups;

CREATE TEMP TABLE project_status_index_migration_backup AS
SELECT project_id, source_id, comparison_branch, branch, commits_ahead,
       tracked_changes, untracked_changes, checked_at
FROM project_status_index;

CREATE TEMP TABLE recent_render_project_migration_backup AS
SELECT id, project_id FROM recent_renders WHERE project_id IS NOT NULL;
UPDATE recent_renders SET project_id = NULL WHERE project_id IS NOT NULL;

CREATE TABLE projects_next (
    [id] TEXT PRIMARY KEY
        CHECK (length([id]) BETWEEN 2 AND 4 AND [id] NOT GLOB '*[^A-Z]*'),
    [source_id] INTEGER NOT NULL UNIQUE
        REFERENCES project_sources ([source_id]),
    [title] TEXT NOT NULL COLLATE NOCASE UNIQUE
        CHECK ([title] = trim([title]) AND length([title]) > 0),
    [git_remote] TEXT
        CHECK ([git_remote] IS NULL OR ([git_remote] = trim([git_remote]) AND length([git_remote]) > 0)),
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
        CHECK (strftime('%Y-%m-%dT%H:%M:%fZ', [unmanaged_at]) IS [unmanaged_at]),
    [comparison_branch] TEXT NOT NULL DEFAULT 'main'
        CHECK (length([comparison_branch]) BETWEEN 1 AND 1024 AND [comparison_branch] = trim([comparison_branch]))
) STRICT;

INSERT INTO projects_next (
    id, source_id, title, git_remote, color, export_include_in_all,
    created_at, paused_at, unmanaged_at, comparison_branch
)
SELECT
    id, source_id, title, git_remote, color, export_include_in_all,
    created_at, paused_at, unmanaged_at, comparison_branch
FROM projects;

DROP TABLE projects;
ALTER TABLE projects_next RENAME TO projects;

INSERT INTO project_groups (project_id, group_name)
SELECT project_id, group_name FROM project_groups_migration_backup;
DROP TABLE project_groups_migration_backup;

INSERT INTO project_status_index (
    project_id, source_id, comparison_branch, branch, commits_ahead,
    tracked_changes, untracked_changes, checked_at
)
SELECT
    project_id, source_id, comparison_branch, branch, commits_ahead,
    tracked_changes, untracked_changes, checked_at
FROM project_status_index_migration_backup;
DROP TABLE project_status_index_migration_backup;

UPDATE recent_renders
SET project_id = (
    SELECT backup.project_id
    FROM recent_render_project_migration_backup AS backup
    WHERE backup.id = recent_renders.id
)
WHERE id IN (SELECT id FROM recent_render_project_migration_backup);
DROP TABLE recent_render_project_migration_backup;

CREATE TEMP TABLE project_migration_foreign_key_check (
    valid INTEGER CHECK (valid = 1)
);
INSERT INTO project_migration_foreign_key_check (valid)
SELECT CASE WHEN EXISTS (SELECT 1 FROM pragma_foreign_key_check) THEN 0 ELSE 1 END;
DROP TABLE project_migration_foreign_key_check;

CREATE VIEW active_projects AS
SELECT
    projects.[id],
    projects.[title],
    project_sources.[source_kind],
    project_sources.[source_value],
    projects.[git_remote],
    projects.[color],
    projects.[created_at]
FROM projects
JOIN project_sources USING ([source_id])
WHERE projects.[paused_at] IS NULL
  AND projects.[unmanaged_at] IS NULL;
