//! The `diff://<repo-id>/<content-hash>` scheme (ADR-0004). The handler is the
//! sandbox: only shape-valid ids/hashes resolving under `<store>/diffs` are served.
use std::path::{Path, PathBuf};

/// A store id/hash is 16 lowercase hex chars (see `gtl_store::id`). Reject anything
/// else so `..`, separators, and absolute paths can never reach the filesystem.
fn is_store_token(s: &str) -> bool {
    s.len() == 16
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Resolve a `diff://` request to a store `.html` path, or `None` if the request
/// is malformed or would escape the store. Pure: no I/O beyond joining paths.
pub fn resolve_diff_uri(store_root: &Path, repo_id: &str, content_hash: &str) -> Option<PathBuf> {
    if !is_store_token(repo_id) || !is_store_token(content_hash) {
        return None;
    }
    let path = store_root
        .join("diffs")
        .join(repo_id)
        .join(format!("{content_hash}.html"));
    // Belt-and-braces: the token check already forbids separators, but assert the
    // lexical prefix so a future token-shape change can't silently open the gate.
    let root = store_root.join("diffs");
    path.starts_with(&root).then_some(path)
}

/// The store root the viewer serves from: `GIT_TOOLS_DATA_DIR` override, else the
/// platform project-data dir — the same resolution the CLI uses (Phase 1).
pub fn store_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GIT_TOOLS_DATA_DIR") {
        return Some(PathBuf::from(p));
    }
    directories::ProjectDirs::from("", "", "git-tools").map(|d| d.data_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef";
    const HASH: &str = "fedcba9876543210";

    #[test]
    fn resolves_a_valid_id_and_hash() {
        let got = resolve_diff_uri(Path::new("/store"), ID, HASH).unwrap();
        assert_eq!(
            got,
            PathBuf::from("/store/diffs/0123456789abcdef/fedcba9876543210.html")
        );
    }

    #[test]
    fn rejects_path_traversal_and_separators() {
        assert!(resolve_diff_uri(Path::new("/store"), "..", HASH).is_none());
        assert!(resolve_diff_uri(Path::new("/store"), ID, "../../etc/passwd").is_none());
        assert!(resolve_diff_uri(Path::new("/store"), "a/b/c", HASH).is_none());
    }

    #[test]
    fn rejects_wrong_length_or_non_hex_or_uppercase() {
        assert!(resolve_diff_uri(Path::new("/store"), "deadbeef", HASH).is_none()); // too short
        assert!(resolve_diff_uri(Path::new("/store"), ID, "zzzzzzzzzzzzzzzz").is_none()); // non-hex
        assert!(resolve_diff_uri(Path::new("/store"), "0123456789ABCDEF", HASH).is_none()); // uppercase
    }

    #[test]
    fn rejects_16char_repo_id_with_path_significant_chars() {
        // 16 chars but contains `/` — hex-digit gate (not length gate) rejects it.
        assert!(resolve_diff_uri(Path::new("/s"), "0123456789abc/ef", HASH).is_none());
        // 16 chars but contains `..` — hex-digit gate rejects it.
        assert!(resolve_diff_uri(Path::new("/s"), "0123456789ab..ef", HASH).is_none());
    }
}
