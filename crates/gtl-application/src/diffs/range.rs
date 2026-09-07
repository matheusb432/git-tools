use gtl_models::git::{GitDiffSpec, GitRange, GitRevision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiffRanges {
    pub(super) diff: GitDiffSpec,
    pub(super) log: Option<GitRange>,
}

impl DiffRanges {
    pub(super) fn unpushed(base: &GitRevision) -> Self {
        Self::exact(GitRange::two_dot(base, &GitRevision::head()))
    }

    pub(super) fn working_tree(base: &GitRevision) -> Self {
        Self {
            diff: GitDiffSpec::AgainstWorkingTree(base.clone()),
            log: Some(GitRange::two_dot(base, &GitRevision::head())),
        }
    }

    pub(super) fn merge(base: &GitRevision) -> Self {
        Self {
            diff: GitDiffSpec::Range(GitRange::three_dot(base, &GitRevision::head())),
            log: Some(GitRange::two_dot(base, &GitRevision::head())),
        }
    }

    pub(super) fn exact(range: GitRange) -> Self {
        Self {
            diff: GitDiffSpec::Range(range.clone()),
            log: Some(range),
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::git::{GitDiffSpec, GitRange, GitRevision};

    use super::DiffRanges;

    #[test]
    fn base_derived_ranges_have_explicit_shapes() {
        for (actual, expected_diff, expected_log) in [
            (
                DiffRanges::unpushed(&GitRevision::main()),
                "main..HEAD",
                "main..HEAD",
            ),
            (
                DiffRanges::working_tree(&GitRevision::main()),
                "main",
                "main..HEAD",
            ),
            (
                DiffRanges::merge(&GitRevision::main()),
                "main...HEAD",
                "main..HEAD",
            ),
        ] {
            assert_eq!(actual.diff.as_arg(), expected_diff);
            assert_eq!(actual.log.as_ref().unwrap().as_ref(), expected_log);
        }
    }

    #[test]
    fn exact_range_is_used_verbatim() {
        let actual = DiffRanges::exact(GitRange::try_new("aaa..bbb").unwrap());

        assert_eq!(actual.diff, GitDiffSpec::Range(actual.log.clone().unwrap()));
        assert_eq!(actual.log.as_ref().unwrap().as_ref(), "aaa..bbb");
    }
}
