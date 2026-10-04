CREATE TABLE github_compare_diffs (
    id INTEGER PRIMARY KEY,
    repository TEXT NOT NULL CHECK (length(repository) > 0),
    base TEXT NOT NULL CHECK (length(base) > 0),
    head TEXT NOT NULL CHECK (length(head) > 0),
    text_id TEXT NOT NULL
        CHECK (length(text_id) = 64 AND text_id NOT GLOB '*[^0-9a-f]*'),
    used_order INTEGER NOT NULL,
    UNIQUE (repository, base, head)
) STRICT;
