-- The viewer saves one extension filter per repository root; it replaces the
-- diff.exclude settings that config.toml carried. An inactive filter has no row.
CREATE TABLE repository_extension_filters (
    repository_root TEXT PRIMARY KEY NOT NULL CHECK (length(repository_root) > 0),
    mode TEXT NOT NULL CHECK (mode IN ('hide', 'only')),
    extensions_json TEXT NOT NULL CHECK (
        json_valid(extensions_json)
        AND json_type(extensions_json) = 'array'
        AND json_array_length(extensions_json) > 0
    )
) STRICT;
