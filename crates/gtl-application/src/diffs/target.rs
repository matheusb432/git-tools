use std::num::NonZeroU32;

pub use gtl_models::diffs::PinnedRange;
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
    /// A three-dot merge diff against the base revision.
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

/// A validated diff target resolved from an invocation request.
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
    /// A three-dot merge diff against the base branch.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_range_display_uses_the_supported_ten_character_width() {
        let pin = crate::testing::pinned_range(
            "1111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222",
        );
        assert_eq!(pin.display_base(), "1111111111");
        assert_eq!(pin.display_range(), "1111111111..2222222222");
    }

    #[test]
    fn zero_last_count_is_rejected_with_the_request_contract_message() {
        let error = DiffTarget::try_from(DiffTargetRequest::Last { count: 0 })
            .expect_err("zero cannot form a valid last-count target");

        assert_eq!(error.to_string(), "last count must be >= 1");
    }

    #[test]
    fn models_targets_map_back_to_their_symbolic_requests() {
        let pinned_range = Some(crate::testing::pinned_range(
            "1111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222",
        ));
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
