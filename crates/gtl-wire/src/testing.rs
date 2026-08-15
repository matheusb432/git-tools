use gtl_models::diffs::{CommitId, PinnedRange};

pub(crate) fn commit_id(raw: impl AsRef<str>) -> CommitId {
    raw.as_ref().try_into().expect("fixture commit ID is valid")
}

pub(crate) fn pinned_range(base: impl AsRef<str>, head: impl AsRef<str>) -> PinnedRange {
    PinnedRange {
        base: commit_id(base),
        head: commit_id(head),
    }
}
