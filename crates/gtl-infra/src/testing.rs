use gtl_models::diffs::{CommitId, PinnedRange};

pub(crate) fn commit_id(seed: &str) -> CommitId {
    seed.chars()
        .cycle()
        .take(40)
        .collect::<String>()
        .try_into()
        .expect("fixture commit ID is valid")
}

pub(crate) fn pinned_range(base: &str, head: &str) -> PinnedRange {
    PinnedRange {
        base: commit_id(base),
        head: commit_id(head),
    }
}
