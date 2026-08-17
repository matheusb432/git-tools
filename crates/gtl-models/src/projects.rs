//! Repository information supplied by the active sample_project project catalog.

use crate::{
    git::RemoteUrl,
    paths::{ProjectName, RepositoryRoot},
};

pub mod push_ledger;

/// One active sample_project project resolved to an absolute local Git repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRepository {
    pub name: ProjectName,
    pub path: RepositoryRoot,
    pub remote: Option<RemoteUrl>,
}
