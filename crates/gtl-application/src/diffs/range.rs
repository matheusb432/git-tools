#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiffRanges {
    pub(super) diff: String,
    pub(super) log: String,
}

impl DiffRanges {
    pub(super) fn unpushed(base: &str) -> Self {
        Self::exact(format!("{base}..HEAD"))
    }

    pub(super) fn working_tree(base: &str) -> Self {
        Self {
            diff: base.to_string(),
            log: format!("{base}..HEAD"),
        }
    }

    pub(super) fn merge(base: &str) -> Self {
        Self {
            diff: format!("{base}...HEAD"),
            log: format!("{base}..HEAD"),
        }
    }

    pub(super) fn exact(range: impl Into<String>) -> Self {
        let range = range.into();
        Self {
            diff: range.clone(),
            log: range,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DiffRanges;

    #[test]
    fn base_derived_ranges_have_explicit_shapes() {
        for (actual, expected_diff, expected_log) in [
            (DiffRanges::unpushed("main"), "main..HEAD", "main..HEAD"),
            (DiffRanges::working_tree("main"), "main", "main..HEAD"),
            (DiffRanges::merge("main"), "main...HEAD", "main..HEAD"),
        ] {
            assert_eq!(actual.diff, expected_diff);
            assert_eq!(actual.log, expected_log);
        }
    }

    #[test]
    fn exact_range_is_used_verbatim() {
        let actual = DiffRanges::exact("aaa..bbb");

        assert_eq!(actual.diff, "aaa..bbb");
        assert_eq!(actual.log, "aaa..bbb");
    }
}
