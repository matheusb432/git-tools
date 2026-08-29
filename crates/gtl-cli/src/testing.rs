use gtl_models::{
    diffs::CommitId,
    git::{BranchName, CommitCount, GitObjectId, RemoteName, RemoteUrl, TagName},
    paths::{ProjectName, RepositoryRelativePath, RepositoryRoot},
    repository::PathCount,
};

pub(crate) fn project_name(raw: &str) -> ProjectName {
    ProjectName::try_from(raw).unwrap()
}

pub(crate) fn repository_root(raw: &str) -> RepositoryRoot {
    RepositoryRoot::try_new(raw.into()).unwrap()
}

pub(crate) fn repository_relative_path(raw: &str) -> RepositoryRelativePath {
    RepositoryRelativePath::try_new(raw.into()).unwrap()
}

pub(crate) fn branch_name(raw: &str) -> BranchName {
    BranchName::try_new(raw.to_owned()).unwrap()
}

pub(crate) fn tag_name(raw: &str) -> TagName {
    TagName::try_new(raw.to_owned()).unwrap()
}

pub(crate) fn remote_name(raw: &str) -> RemoteName {
    RemoteName::try_new(raw.to_owned()).unwrap()
}

pub(crate) fn remote_url(raw: &str) -> RemoteUrl {
    RemoteUrl::try_new(raw.to_owned()).unwrap()
}

pub(crate) fn commit_id(seed: &str) -> CommitId {
    seed.chars()
        .cycle()
        .take(40)
        .collect::<String>()
        .try_into()
        .unwrap()
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
