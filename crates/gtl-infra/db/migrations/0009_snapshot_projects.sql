ALTER TABLE recent_renders ADD COLUMN project_id TEXT REFERENCES projects (id);
CREATE INDEX recent_renders_project_id_idx ON recent_renders (project_id, id DESC);
