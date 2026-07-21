//! Resolve a source file's line-comment leader from its extension.
//!
//! The diff preview's "copy code" button prepends a commented context line
//! (`<leader> * <path>, lines: X..Y`). Each [`CommentAdapter`] claims the
//! extensions that share one leader; unknown or extension-less paths fall back
//! to [`DEFAULT_LEADER`] (`//`).

use std::path::Path;

/// Leader used when no adapter claims the extension (and for extension-less paths).
const DEFAULT_LEADER: &str = "//";

/// One language family's line-comment leader plus the extensions it owns.
struct CommentAdapter {
    leader: &'static str,
    extensions: &'static [&'static str],
}

// ! Add a language by extending an adapter's extension list or appending a new adapter.
const ADAPTERS: &[CommentAdapter] = &[
    CommentAdapter {
        leader: "//",
        extensions: &["rs", "ts", "tsx", "js", "jsx", "cs"],
    },
    CommentAdapter {
        leader: "#",
        extensions: &["sh"],
    },
];

/// The line-comment leader for `path`, matched case-insensitively on its extension.
pub(crate) fn comment_leader(path: &str) -> &'static str {
    let Some(ext) = Path::new(path).extension().and_then(|ext| ext.to_str()) else {
        return DEFAULT_LEADER;
    };
    ADAPTERS
        .iter()
        .find(|adapter| {
            adapter
                .extensions
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        })
        .map_or(DEFAULT_LEADER, |adapter| adapter.leader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_languages_use_double_slash() {
        for path in ["src/lib.rs", "app.ts", "view.tsx", "a.js", "b.jsx", "P.cs"] {
            assert_eq!(comment_leader(path), "//", "leader for {path}");
        }
    }

    #[test]
    fn shell_scripts_use_hash() {
        assert_eq!(comment_leader("xtask/bootstrap.sh"), "#");
    }

    #[test]
    fn extension_matching_is_case_insensitive() {
        assert_eq!(comment_leader("MAIN.RS"), "//");
        assert_eq!(comment_leader("BUILD.SH"), "#");
    }

    #[test]
    fn unknown_and_missing_extensions_fall_back_to_double_slash() {
        assert_eq!(comment_leader("data.toml"), "//");
        assert_eq!(comment_leader("Makefile"), "//");
        assert_eq!(comment_leader("path/to/dir"), "//");
        assert_eq!(comment_leader(""), "//");
    }
}
