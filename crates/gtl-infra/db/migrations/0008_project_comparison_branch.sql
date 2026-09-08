ALTER TABLE projects ADD COLUMN comparison_branch TEXT NOT NULL DEFAULT 'main'
    CHECK (length(comparison_branch) BETWEEN 1 AND 1024 AND comparison_branch = trim(comparison_branch));
