use std::num::NonZeroU32;

/// What a `diff` invocation targets, resolved from its optional positional argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTarget {
    /// Unpushed work: `@{u}..HEAD`.
    Unpushed,
    /// A single base commit diffed against the working tree.
    Base(String),
    /// An exact `<start>..<end>` commit range.
    Range(String),
    /// A three-dot merge preview against the base branch.
    Merge(String),
    /// The last N commits (`HEAD~N..HEAD`).
    Last(NonZeroU32),
}

impl DiffTarget {
    /// Resolves the `diff` positional into a target: empty/absent is unpushed work, a value
    /// containing `..` is an exact range, anything else is a base commit.
    pub fn from_arg(arg: Option<&str>) -> Self {
        match arg {
            None => Self::Unpushed,
            Some(value) if value.trim().is_empty() => Self::Unpushed,
            Some(value) if value.contains("..") => Self::Range(value.to_string()),
            Some(value) => Self::Base(value.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_target_empty_or_absent_is_unpushed() {
        assert_eq!(DiffTarget::from_arg(None), DiffTarget::Unpushed);
        assert_eq!(DiffTarget::from_arg(Some("")), DiffTarget::Unpushed);
        assert_eq!(DiffTarget::from_arg(Some("   ")), DiffTarget::Unpushed);
    }

    #[test]
    fn diff_target_distinguishes_range_from_base() {
        assert_eq!(
            DiffTarget::from_arg(Some("abc123..def456")),
            DiffTarget::Range("abc123..def456".to_string())
        );
        assert_eq!(
            DiffTarget::from_arg(Some("abc123")),
            DiffTarget::Base("abc123".to_string())
        );
    }
}
