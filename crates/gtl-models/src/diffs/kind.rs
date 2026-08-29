use serde::{Deserialize, Serialize};

/// How the new side of a diff was produced. `TwoDot`/`ThreeDot` are pure commit
/// ranges (range-lookup eligible); `WorkTree` includes uncommitted changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    TwoDot,
    ThreeDot,
    WorkTree,
}

impl DiffKind {
    /// Classify from a diff-range string: `...` ⇒ three-dot, `..` ⇒ two-dot,
    /// neither ⇒ working tree (matches the application diff target decision).
    #[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_kind_classifies_range_operators() {
        assert_eq!(DiffKind::from_diff_range("main...HEAD"), DiffKind::ThreeDot);
        assert_eq!(DiffKind::from_diff_range("main..HEAD"), DiffKind::TwoDot);
        assert_eq!(DiffKind::from_diff_range("abc123"), DiffKind::WorkTree);
    }
}
