use gtl_models::{
    diffs::{Commit, CommitId, PinnedRange},
    git::{BranchName, GitHead, GitRevision},
    paths::{ProjectName, RepositoryRelativePath, RepositoryRoot},
    timestamps::MachineTimestamp,
};

pub(crate) fn project_name(raw: &str) -> ProjectName {
    ProjectName::try_from(raw).expect("fixture project name is non-empty")
}

pub(crate) fn repository_root(raw: &str) -> RepositoryRoot {
    RepositoryRoot::try_new(raw.into()).expect("fixture repository root is absolute")
}

pub(crate) fn repository_relative_path(raw: &str) -> RepositoryRelativePath {
    RepositoryRelativePath::try_new(raw.into()).expect("fixture repository-relative path is valid")
}

pub(crate) fn git_head(raw: &str) -> GitHead {
    GitHead::Branch(BranchName::try_new(raw.to_owned()).expect("fixture branch name is non-empty"))
}

pub(crate) fn git_revision(raw: &str) -> GitRevision {
    GitRevision::try_new(raw.to_owned()).expect("fixture Git revision is non-empty")
}

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
        committed_at: MachineTimestamp::try_from("2026-01-01T00:00:00Z")
            .expect("fixture commit timestamp is valid"),
        parents: Vec::new(),
    }
}

pub(crate) fn pinned_range(base: &str, head: &str) -> PinnedRange {
    PinnedRange {
        base: commit_id(base),
        head: commit_id(head),
    }
}
