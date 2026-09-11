CREATE TABLE pinned_viewer_tabs (
    position INTEGER PRIMARY KEY,
    recipe_json TEXT NOT NULL CHECK (json_valid(recipe_json)),
    live INTEGER NOT NULL CHECK (live IN (0, 1))
) STRICT;

DELETE FROM live_views WHERE comparison = 'local_changes'
    AND EXISTS (SELECT 1 FROM live_views AS other
        WHERE other.source_kind = live_views.source_kind
          AND other.source_value = live_views.source_value
          AND other.comparison = 'unpushed_commits');
UPDATE live_views SET comparison = 'unpushed_commits' WHERE comparison = 'local_changes';
