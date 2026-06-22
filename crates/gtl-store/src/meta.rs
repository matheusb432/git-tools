//! Per-artifact metadata (sidecar) and the diff kind that keys range lookups.
use serde::{Deserialize, Serialize};

/// How the new side of a diff was produced. `TwoDot`/`ThreeDot` are pure commit
/// ranges (range-lookup eligible); `WorkTree` includes uncommitted changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    TwoDot,
    ThreeDot,
    WorkTree,
}

impl DiffKind {
    /// Classify from a diff-range string: `...` ⇒ three-dot, `..` ⇒ two-dot,
    /// neither ⇒ working tree (matches `crate::diff::blame_targets`).
    pub fn from_diff_range(diff_range: &str) -> DiffKind {
        if diff_range.contains("...") {
            DiffKind::ThreeDot
        } else if diff_range.contains("..") {
            DiffKind::TwoDot
        } else {
            DiffKind::WorkTree
        }
    }
}

/// Metadata stored alongside each artifact as `<hash>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    pub repo_id: String,
    pub repo_name: String,
    pub repo_root: String,
    pub kind: DiffKind,
    pub base_sha: String,
    pub head_sha: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub generated_at: String,
    pub title: String,
    pub byte_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_kind_classifies_range_operators() {
        assert_eq!(DiffKind::from_diff_range("main...HEAD"), DiffKind::ThreeDot);
        assert_eq!(DiffKind::from_diff_range("main..HEAD"), DiffKind::TwoDot);
        assert_eq!(DiffKind::from_diff_range("abc123"), DiffKind::WorkTree);
    }

    #[test]
    fn sidecar_round_trips_through_json() {
        let sc = Sidecar {
            repo_id: "deadbeef00000000".into(),
            repo_name: "git-tools".into(),
            repo_root: "/home/u/tools/git-tools".into(),
            kind: DiffKind::TwoDot,
            base_sha: "aaaa".into(),
            head_sha: "bbbb".into(),
            range_label: "origin/main..HEAD".into(),
            head_committed_at: "2026-06-22T10:00:00Z".into(),
            generated_at: "2026-06-22T10:01:00Z".into(),
            title: "diff".into(),
            byte_size: 1234,
        };
        let json = serde_json::to_string(&sc).unwrap();
        assert_eq!(serde_json::from_str::<Sidecar>(&json).unwrap(), sc);
    }
}
