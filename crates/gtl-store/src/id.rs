//! Stable identities for the store: a repo id and an artifact content hash.
use std::path::Path;

use sha2::{Digest, Sha256};

/// First 16 hex chars of the SHA-256 of `input`.
fn short_sha256(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut out = String::with_capacity(16);
    for byte in &digest[..8] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// A stable id for a repo. Prefers the root-commit sha (stable across
/// clone/move/rename); falls back to hashing the canonical path for a repo with
/// no commits. Always 16 hex chars.
pub fn repo_id(root_commit: Option<&str>, canonical_path: &Path) -> String {
    match root_commit {
        Some(sha) if !sha.trim().is_empty() => short_sha256(sha.trim()),
        _ => short_sha256(&canonical_path.to_string_lossy()),
    }
}

/// Content address of a rendered artifact: 16 hex chars over the exact HTML.
pub fn content_hash(html: &str) -> String {
    short_sha256(html)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn content_hash_is_stable_and_distinct() {
        assert_eq!(
            content_hash("<html>a</html>"),
            content_hash("<html>a</html>")
        );
        assert_ne!(
            content_hash("<html>a</html>"),
            content_hash("<html>b</html>")
        );
        assert_eq!(content_hash("x").len(), 16);
    }

    #[test]
    fn repo_id_uses_root_commit_and_is_path_independent() {
        let from_sha = repo_id(Some("abc123"), &PathBuf::from("/tmp/repo"));
        let from_path = repo_id(None, &PathBuf::from("/tmp/repo"));
        assert_ne!(from_sha, from_path);
        assert_eq!(
            from_sha,
            repo_id(Some("abc123"), &PathBuf::from("/elsewhere"))
        );
    }

    #[test]
    fn repo_id_falls_back_to_path_when_no_root_commit() {
        assert_eq!(
            repo_id(None, &PathBuf::from("/tmp/repo")),
            repo_id(Some("   "), &PathBuf::from("/tmp/repo")),
        );
    }
}
