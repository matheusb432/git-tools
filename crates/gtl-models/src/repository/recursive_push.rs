use crate::{
    git::{BranchName, RemoteName},
    paths::{ProjectName, RepositoryRoot},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    Push {
        branch: BranchName,
        remote: RemoteName,
    },
    /// Upstream exists and `@{u}..HEAD` is empty.
    Synced {
        branch: BranchName,
        remote: RemoteName,
    },
    Skip {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoTarget {
    pub path: RepositoryRoot,
    pub label: ProjectName,
    pub dest: Dest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubreposPlan {
    Refused(String),
    Ready(Vec<RepoTarget>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoOutcome {
    Pushed,
    UpToDate,
    Skipped(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoReport {
    pub label: ProjectName,
    pub outcome: RepoOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Partial,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushAllResult {
    pub status: Status,
    pub reports: Vec<RepoReport>,
}
