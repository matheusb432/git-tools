use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

/// The unchecked request form of a diff target selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffTargetRequest {
    /// Commits ahead of the configured upstream.
    Unpushed,
    /// The working tree relative to one base revision.
    Base { rev: String },
    /// An exact two-dot revision range.
    Range { range: String },
    /// A three-dot merge preview against the base revision.
    Merge { base: String },
    /// The latest `count` commits.
    Last { count: u32 },
}

/// A semantic failure converting an unchecked diff target request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiffTargetRequestError {
    /// A latest-commit selection cannot contain zero commits.
    #[error("last count must be >= 1")]
    LastCountZero,
}

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

impl TryFrom<DiffTargetRequest> for DiffTarget {
    type Error = DiffTargetRequestError;

    fn try_from(request: DiffTargetRequest) -> Result<Self, Self::Error> {
        Ok(match request {
            DiffTargetRequest::Unpushed => Self::Unpushed { pinned: None },
            DiffTargetRequest::Base { rev } => Self::Base(rev),
            DiffTargetRequest::Range { range } => Self::Range {
                range,
                pinned: None,
            },
            DiffTargetRequest::Merge { base } => Self::Merge { base, pinned: None },
            DiffTargetRequest::Last { count } => Self::Last {
                count: NonZeroU32::new(count).ok_or(DiffTargetRequestError::LastCountZero)?,
                pinned: None,
            },
        })
    }
}

impl From<&DiffTarget> for DiffTargetRequest {
    /// Preserve the symbolic selection for execution by the daemon. Immutable
    /// pins belong to stored recipes and are intentionally not part of this request.
    fn from(target: &DiffTarget) -> Self {
        match target {
            DiffTarget::Unpushed { .. } => Self::Unpushed,
            DiffTarget::Base(rev) => Self::Base { rev: rev.clone() },
            DiffTarget::Range { range, .. } => Self::Range {
                range: range.clone(),
            },
            DiffTarget::Merge { base, .. } => Self::Merge { base: base.clone() },
            DiffTarget::Last { count, .. } => Self::Last { count: count.get() },
        }
    }
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

    #[test]
    fn zero_last_count_is_rejected_with_the_request_contract_message() {
        let error = DiffTarget::try_from(DiffTargetRequest::Last { count: 0 })
            .expect_err("zero cannot form a valid last-count target");

        assert_eq!(error.to_string(), "last count must be >= 1");
    }

    #[test]
    fn domain_targets_map_back_to_their_symbolic_requests() {
        let pinned_range = Some(PinnedRange {
            base: "base-sha".into(),
            head: "head-sha".into(),
        });
        let cases = [
            (
                DiffTarget::Unpushed {
                    pinned: pinned_range.clone(),
                },
                DiffTargetRequest::Unpushed,
            ),
            (
                DiffTarget::Base("main".into()),
                DiffTargetRequest::Base { rev: "main".into() },
            ),
            (
                DiffTarget::Range {
                    range: "main..feature".into(),
                    pinned: pinned_range.clone(),
                },
                DiffTargetRequest::Range {
                    range: "main..feature".into(),
                },
            ),
            (
                DiffTarget::Merge {
                    base: "main".into(),
                    pinned: pinned_range.clone(),
                },
                DiffTargetRequest::Merge {
                    base: "main".into(),
                },
            ),
            (
                DiffTarget::Last {
                    count: NonZeroU32::new(3).unwrap(),
                    pinned: pinned_range,
                },
                DiffTargetRequest::Last { count: 3 },
            ),
        ];

        for (target, expected) in cases {
            assert_eq!(DiffTargetRequest::from(&target), expected);
        }
    }
}
