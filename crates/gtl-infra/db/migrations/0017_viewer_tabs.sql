-- The viewer saves every open tab, not only pinned ones, so a restarted server
-- restores the whole tab strip in order. A tab without a saved label shows its
-- recipe's pending label until it renders. Saved live views become unpinned
-- live tabs unless a pinned live tab already follows the same repository.
CREATE TABLE viewer_tabs (
    position INTEGER PRIMARY KEY CHECK (position >= 0),
    recipe_json TEXT NOT NULL CHECK (json_valid(recipe_json)),
    label_json TEXT CHECK (label_json IS NULL OR json_valid(label_json)),
    pinned INTEGER NOT NULL CHECK (pinned IN (0, 1)),
    live INTEGER NOT NULL CHECK (live IN (0, 1)),
    active INTEGER NOT NULL CHECK (active IN (0, 1))
) STRICT;

CREATE UNIQUE INDEX viewer_tabs_active_idx ON viewer_tabs (active) WHERE active = 1;

INSERT INTO viewer_tabs (position, recipe_json, label_json, pinned, live, active)
SELECT position, recipe_json, NULL, 1, live, 0
FROM pinned_viewer_tabs;

INSERT INTO viewer_tabs (position, recipe_json, label_json, pinned, live, active)
SELECT
    (SELECT coalesce(max(position) + 1, 0) FROM pinned_viewer_tabs)
        + row_number() OVER (ORDER BY view.id) - 1,
    json_object(
        'source', json_object('kind', 'local_repo', 'value', view.source_value),
        'op', json_object(
            'op', 'diff',
            'target', CASE view.comparison
                WHEN 'local_changes' THEN json_object('target', 'base', 'rev', 'HEAD')
                ELSE json_object('target', 'unpushed')
            END
        )
    ),
    NULL, 0, 1, 0
FROM live_views AS view
WHERE view.source_kind = 'LocalRepo'
  AND NOT EXISTS (
    SELECT 1 FROM pinned_viewer_tabs AS pinned
    WHERE pinned.live = 1
      AND json_extract(pinned.recipe_json, '$.source.value') = view.source_value
      AND json_extract(pinned.recipe_json, '$.op.op') = 'diff'
      AND json_extract(pinned.recipe_json, '$.op.target.target') = CASE view.comparison
          WHEN 'local_changes' THEN 'base'
          ELSE 'unpushed'
      END
  );

DROP TABLE pinned_viewer_tabs;
