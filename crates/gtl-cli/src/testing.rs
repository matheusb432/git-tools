use gtl_models::{
    diffs::{CommitId, PinnedRange},
    git::{
        BranchName, CommitCount, GitObjectId, GitRange, GitRevision, RemoteName, RemoteUrl, TagName,
    },
    paths::{ProjectName, RepositoryRelativePath, RepositoryRoot},
    repository::PathCount,
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

pub(crate) fn branch_name(raw: &str) -> BranchName {
    BranchName::try_new(raw.to_owned()).expect("fixture branch name is non-empty")
}

pub(crate) fn tag_name(raw: &str) -> TagName {
    TagName::try_new(raw.to_owned()).expect("fixture tag name is non-empty")
}

pub(crate) fn remote_name(raw: &str) -> RemoteName {
    RemoteName::try_new(raw.to_owned()).expect("fixture remote name is non-empty")
}

pub(crate) fn remote_url(raw: &str) -> RemoteUrl {
    RemoteUrl::try_new(raw.to_owned()).expect("fixture remote URL is non-empty")
}

pub(crate) fn git_revision(raw: &str) -> GitRevision {
    GitRevision::try_new(raw.to_owned()).expect("fixture Git revision is non-empty")
}

pub(crate) fn git_range(raw: &str) -> GitRange {
    GitRange::try_new(raw.to_owned()).expect("fixture Git range is non-empty")
}

pub(crate) fn commit_id(seed: &str) -> CommitId {
    seed.chars()
        .cycle()
        .take(40)
        .collect::<String>()
        .try_into()
        .expect("fixture commit ID is valid")
}

pub(crate) fn git_object_id(seed: &str) -> GitObjectId {
    GitObjectId::from(&commit_id(seed))
}

pub(crate) const fn path_count(value: u64) -> PathCount {
    PathCount::new(value)
}

pub(crate) const fn commit_count(value: u64) -> CommitCount {
    CommitCount::new(value)
}

pub(crate) fn pinned_range(base: &str, head: &str) -> PinnedRange {
    PinnedRange {
        base: commit_id(base),
        head: commit_id(head),
    }
}
