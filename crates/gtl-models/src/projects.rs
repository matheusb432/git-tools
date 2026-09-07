//! Repository information supplied by the active GTL project catalog.

use crate::{
    git::RemoteUrl,
    paths::{ProjectName, RepositoryRoot},
};

pub mod catalogue;
pub mod push_ledger;

/// One active project resolved to an absolute local Git repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRepository {
    pub name: ProjectName,
    pub path: RepositoryRoot,
    pub remote: Option<RemoteUrl>,
}
