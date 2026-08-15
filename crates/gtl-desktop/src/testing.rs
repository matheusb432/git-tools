use gtl_models::diffs::{Commit, CommitId, PinnedRange};

pub(crate) fn commit_id(seed: &str) -> CommitId {
    seed.chars()
        .cycle()
        .take(40)
        .collect::<String>()
        .try_into()
        .expect("fixture commit ID is valid")
}

pub(crate) fn commit(id: &str, subject: impl Into<String>) -> Commit {
    Commit {
        id: commit_id(id),
        subject: subject.into(),
        body: String::new(),
        date: String::new(),
        iso: String::new(),
        parents: Vec::new(),
    }
}

pub(crate) fn pinned_range(base: &str, head: &str) -> PinnedRange {
    PinnedRange {
        base: commit_id(base),
        head: commit_id(head),
    }
}
