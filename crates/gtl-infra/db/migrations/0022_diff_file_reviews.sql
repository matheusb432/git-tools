CREATE TABLE diff_file_reviews (
    id INTEGER PRIMARY KEY,
    repository_root TEXT NOT NULL CHECK (length(repository_root) > 0),
    file_path TEXT NOT NULL CHECK (length(file_path) > 0),
    content_id BLOB NOT NULL CHECK (length(content_id) = 32),
    UNIQUE (repository_root, file_path, content_id)
) STRICT;
