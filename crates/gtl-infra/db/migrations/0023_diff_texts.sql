CREATE TABLE diff_texts (
    id INTEGER PRIMARY KEY,
    text_id TEXT NOT NULL UNIQUE
        CHECK (length(text_id) = 64 AND text_id NOT GLOB '*[^0-9a-f]*'),
    content TEXT NOT NULL
) STRICT;
