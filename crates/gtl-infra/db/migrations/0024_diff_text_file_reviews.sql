CREATE TABLE diff_text_file_reviews (
    id INTEGER PRIMARY KEY,
    file_path TEXT NOT NULL CHECK (length(file_path) > 0),
    content_id BLOB NOT NULL CHECK (length(content_id) = 32),
    UNIQUE (file_path, content_id)
) STRICT;
