use serde::{Deserialize, Serialize};

use super::CommitId;
use crate::paths::ProjectName;

/// Names what one diff view shows, in parts the viewer localizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "title", rename_all = "snake_case")]
pub enum DiffViewTitle {
    /// Changes between revisions, or between a revision and the working tree.
    Diff,
    /// The changes the current branch would merge into its base.
    MergeDiff,
    /// The changes one selected commit introduced.
    Commit { id: CommitId },
    /// A render the user named.
    Named { name: ProjectName },
}
