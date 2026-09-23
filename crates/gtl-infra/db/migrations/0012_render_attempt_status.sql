ALTER TABLE recent_renders
ADD COLUMN render_status TEXT NOT NULL DEFAULT 'success'
    CHECK (render_status IN ('pending', 'success', 'error'));

DROP INDEX recent_renders_fingerprint_idx;
CREATE UNIQUE INDEX recent_renders_fingerprint_idx
ON recent_renders (
    source_id,
    repo_name,
    coalesce(pinned_base, X''),
    coalesce(pinned_head, X'')
)
WHERE render_status = 'success';

CREATE TABLE render_errors (
    id INTEGER PRIMARY KEY,
    recent_render_id INTEGER NOT NULL
        REFERENCES recent_renders (id) ON DELETE CASCADE,
    error_code TEXT NOT NULL
        CHECK (error_code IN (
            'repository_directory_not_found',
            'repository_directory_not_git_repository',
            'source_unavailable',
            'render_failed',
            'publication_failed'
        )),
    error_detail TEXT NOT NULL
        CHECK (length(error_detail) > 0)
) STRICT;

CREATE INDEX render_errors_recent_render_id_idx
ON render_errors (recent_render_id, id);
