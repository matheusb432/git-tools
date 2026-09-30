use gtl_models::{diffs::CommitId, git::GitHead, paths::RepositoryRoot};
use serde::{Deserialize, Serialize};

use super::{ViewerCommitSummary, ViewerViewIdentity};

pub const VIEWER_COMMIT_SEARCH_RESULTS_MAX: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "value", rename_all = "snake_case")]
pub enum ViewerCommitSearchScope {
    Snapshot(ViewerViewIdentity),
    ActiveBranch(RepositoryRoot),
    ActiveBranchSnapshot(ViewerViewIdentity),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchViewerCommits {
    pub scope: ViewerCommitSearchScope,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerCommitSearchResult {
    pub commits: Vec<ViewerCommitSummary>,
    pub total_matches: u64,
    pub branch: Option<GitHead>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerCommit {
    pub scope: ViewerCommitSearchScope,
    pub id: CommitId,
}
