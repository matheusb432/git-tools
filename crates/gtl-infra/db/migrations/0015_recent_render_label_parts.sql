-- Recent renders store the parts a computed label adds to its recipe instead
-- of English title text, so the viewer can localize history labels. The range
-- label keeps only the Git range a successful render compared.
--
-- Dropping recent_renders cascades to render_errors, so its rows are restored
-- from a backup after the copy-and-swap.

CREATE TEMP TABLE render_errors_migration_backup AS
SELECT id, recent_render_id, error_code, error_detail FROM render_errors;

CREATE TABLE recent_renders_next (
  id             INTEGER PRIMARY KEY,
  source_id      INTEGER NOT NULL REFERENCES render_sources (id),
  operation_id   INTEGER NOT NULL REFERENCES render_operations (id),
  target_id      INTEGER REFERENCES render_targets (id),
  argument       TEXT,
  pinned_base    TEXT,
  pinned_head    TEXT,
  recipe_name    TEXT,
  repo_name      TEXT NOT NULL,
  range_label    TEXT,
  commit_count   INTEGER,
  merge_branch   TEXT,
  merge_upstream TEXT,
  rendered_at    TEXT NOT NULL,
  project_id     TEXT REFERENCES projects (id),
  render_status  TEXT NOT NULL DEFAULT 'success'
    CHECK (render_status IN ('pending', 'success', 'error')),
  CHECK ((pinned_base IS NULL) = (pinned_head IS NULL)),
  -- Only the diff operation (seeded id 1) takes a target.
  CHECK ((operation_id = 1) = (target_id IS NOT NULL)),
  -- Only a successful render knows its Git range and label parts.
  CHECK ((render_status = 'success') = (range_label IS NOT NULL)),
  CHECK (render_status = 'success' OR (commit_count IS NULL AND merge_branch IS NULL)),
  -- Unpushed diffs (seeded target id 1) label their commit count.
  CHECK (commit_count IS NULL OR (commit_count >= 0 AND target_id = 1)),
  -- Merge diffs (seeded operation id 2 or target id 4) label both merged heads.
  CHECK ((merge_branch IS NULL) = (merge_upstream IS NULL)),
  CHECK (merge_branch IS NULL OR (
    (operation_id = 2 OR target_id = 4)
    AND length(trim(merge_branch)) > 0
    AND length(trim(merge_upstream)) > 0
  ))
) STRICT;

-- Legacy titles spelled the label parts as '<repo>: N commit(s)' for unpushed
-- diffs and '<repo>: merge <branch>-><upstream>' for merges; other titles
-- leave the parts unknown, so their labels show the recipe alone.
WITH titles AS (
  SELECT id,
         substr(title, 1, length(repo_name) + 2) = repo_name || ': ' AS repository_prefixed,
         substr(title, length(repo_name) + 3) AS rest
  FROM recent_renders
  WHERE render_status = 'success'
),
counts AS (
  SELECT id, CAST(rest AS INTEGER) AS commit_count
  FROM titles
  WHERE repository_prefixed
    AND rest IN (
      CAST(CAST(rest AS INTEGER) AS TEXT) || ' commit',
      CAST(CAST(rest AS INTEGER) AS TEXT) || ' commits'
    )
),
merges AS (
  SELECT id,
         substr(rest, 7, instr(substr(rest, 7), '->') - 1) AS merge_branch,
         substr(rest, 7 + instr(substr(rest, 7), '->') + 1) AS merge_upstream
  FROM titles
  WHERE repository_prefixed
    AND substr(rest, 1, 6) = 'merge '
    AND instr(substr(rest, 7), '->') > 1
)
INSERT INTO recent_renders_next
  (id, source_id, operation_id, target_id, argument, pinned_base, pinned_head,
   recipe_name, repo_name, range_label, commit_count, merge_branch, merge_upstream,
   rendered_at, project_id, render_status)
SELECT r.id, r.source_id, r.operation_id, r.target_id, r.argument,
       r.pinned_base, r.pinned_head, r.recipe_name, r.repo_name,
       CASE WHEN r.render_status = 'success' THEN r.range_label END,
       CASE WHEN r.target_id = 1 AND c.commit_count >= 0 THEN c.commit_count END,
       CASE WHEN m.merge_parsed THEN m.merge_branch END,
       CASE WHEN m.merge_parsed THEN m.merge_upstream END,
       r.rendered_at, r.project_id, r.render_status
FROM recent_renders AS r
LEFT JOIN counts AS c ON c.id = r.id
LEFT JOIN (
  SELECT id, merge_branch, merge_upstream,
         length(trim(merge_branch)) > 0 AND length(trim(merge_upstream)) > 0 AS merge_parsed
  FROM merges
) AS m ON m.id = r.id AND (r.operation_id = 2 OR r.target_id = 4);

DROP TABLE recent_renders;
ALTER TABLE recent_renders_next RENAME TO recent_renders;

CREATE UNIQUE INDEX recent_renders_fingerprint_idx
ON recent_renders (
    source_id,
    repo_name,
    coalesce(pinned_base, X''),
    coalesce(pinned_head, X'')
)
WHERE render_status = 'success';

CREATE INDEX recent_renders_project_id_idx ON recent_renders (project_id, id DESC);

CREATE INDEX recent_renders_repo_name_idx
ON recent_renders (repo_name);

INSERT INTO render_errors (id, recent_render_id, error_code, error_detail)
SELECT id, recent_render_id, error_code, error_detail FROM render_errors_migration_backup;
DROP TABLE render_errors_migration_backup;

CREATE TEMP TABLE recent_render_migration_foreign_key_check (
    valid INTEGER CHECK (valid = 1)
);
INSERT INTO recent_render_migration_foreign_key_check (valid)
SELECT CASE WHEN EXISTS (SELECT 1 FROM pragma_foreign_key_check) THEN 0 ELSE 1 END;
DROP TABLE recent_render_migration_foreign_key_check;
