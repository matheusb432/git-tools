ALTER TABLE recent_renders ADD COLUMN comparison_name TEXT
    CHECK (comparison_name IS NULL OR length(trim(comparison_name)) > 0);
ALTER TABLE viewer_tabs ADD COLUMN comparison_name TEXT
    CHECK (comparison_name IS NULL OR length(trim(comparison_name)) > 0);
ALTER TABLE viewer_tabs ADD COLUMN history_id INTEGER
    REFERENCES recent_renders (id) ON DELETE SET NULL;

UPDATE viewer_tabs SET history_id = (
    SELECT r.id FROM recent_renders r
    JOIN render_sources s ON s.id = r.source_id
    JOIN render_operations o ON o.id = r.operation_id
    LEFT JOIN render_targets t ON t.id = r.target_id
    WHERE r.render_status = 'success'
      AND s.value = json_extract(viewer_tabs.recipe_json, '$.source.value')
      AND o.name = json_extract(viewer_tabs.recipe_json, '$.op.op')
      AND t.name IS json_extract(viewer_tabs.recipe_json, '$.op.target.target')
      AND r.argument IS CAST(coalesce(
          json_extract(viewer_tabs.recipe_json, '$.op.target.rev'),
          json_extract(viewer_tabs.recipe_json, '$.op.target.range'),
          json_extract(viewer_tabs.recipe_json, '$.op.target.base'),
          json_extract(viewer_tabs.recipe_json, '$.op.target.count'),
          json_extract(viewer_tabs.recipe_json, '$.op.base')) AS TEXT)
      AND r.recipe_name IS json_extract(viewer_tabs.recipe_json, '$.name')
      AND r.pinned_base IS coalesce(
          json_extract(viewer_tabs.recipe_json, '$.op.target.pinned.base'),
          json_extract(viewer_tabs.recipe_json, '$.op.pinned.base'))
      AND r.pinned_head IS coalesce(
          json_extract(viewer_tabs.recipe_json, '$.op.target.pinned.head'),
          json_extract(viewer_tabs.recipe_json, '$.op.pinned.head'))
    ORDER BY r.id DESC LIMIT 1
);
