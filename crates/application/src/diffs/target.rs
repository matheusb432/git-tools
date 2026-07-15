use std::num::NonZeroU32;

/// A commit range resolved to immutable SHAs at invocation time. `None` means
/// "resolve symbolically at compute time".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedRange {
    /// Full SHA of the range base (exclusive end).
    pub base: String,
    /// Full SHA of the range head (inclusive end).
    pub head: String,
}

/// What a `diff` invocation targets, resolved from its optional positional argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTarget {
    /// Unpushed work: `@{u}..HEAD`, or the pinned resolution thereof.
    Unpushed { pinned: Option<PinnedRange> },
    /// A single base commit diffed against the working tree (never pinnable).
    Base(String),
    /// An exact `<start>..<end>` commit range.
    Range {
        range: String,
        pinned: Option<PinnedRange>,
    },
    /// A three-dot merge preview against the base branch.
    Merge {
        base: String,
        pinned: Option<PinnedRange>,
    },
    /// The last N commits (`HEAD~N..HEAD`).
    Last {
        count: NonZeroU32,
        pinned: Option<PinnedRange>,
    },
}

impl PinnedRange {
    /// The exact range git computes over.
    pub fn git_range(&self) -> String {
        format!("{}..{}", self.base, self.head)
    }

    /// The abbreviated range shown in the footer/cmd (10-char SHAs — honest
    /// and copy-reproducible, unlike a symbolic range that drifts).
    pub fn display_range(&self) -> String {
        format!("{}..{}", abbrev(&self.base), abbrev(&self.head))
    }

    /// The abbreviated base shown as the view's upstream label.
    pub fn display_base(&self) -> String {
        abbrev(&self.base).to_string()
    }
}

/// First 10 chars of `sha` (char-boundary safe: pinned identities normally
/// hold ASCII hex, but they arrive from untrusted recipe tokens).
fn abbrev(sha: &str) -> &str {
    sha.char_indices()
        .nth(10)
        .map_or(sha, |(index, _)| &sha[..index])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_range_display_is_short_identity_and_char_boundary_safe() {
        let short = PinnedRange {
            base: "abc".into(),
            head: "def".into(),
        };
        assert_eq!(short.display_range(), "abc..def");

        let pin = PinnedRange {
            base: "ééééééééééé".into(),
            head: "abc".into(),
        };
        assert_eq!(pin.display_base(), "éééééééééé");
        assert_eq!(pin.display_range(), "éééééééééé..abc");
    }
}
