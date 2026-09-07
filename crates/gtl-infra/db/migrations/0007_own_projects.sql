ALTER TABLE project_sources RENAME TO render_sources;
DROP INDEX project_sources_value_idx;
CREATE INDEX render_sources_value_idx ON render_sources (value);

CREATE TABLE project_sources (
    [source_id] INTEGER PRIMARY KEY,
    [source_kind] TEXT NOT NULL,
    [source_value] TEXT NOT NULL,
    UNIQUE ([source_kind], [source_value]),
    CHECK ([source_kind] = 'directory')
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
) STRICT;

CREATE TABLE project_groups (
    [project_id] TEXT NOT NULL
        REFERENCES projects ([id]) ON DELETE CASCADE,
    [group_name] TEXT NOT NULL
        CHECK ([group_name] = trim([group_name]) AND length([group_name]) > 0),
    PRIMARY KEY ([project_id], [group_name])
) STRICT;

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
