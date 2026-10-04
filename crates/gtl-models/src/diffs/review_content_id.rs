use serde::{Deserialize, Serialize};

use crate::paths::{RepositoryRelativePath, RepositoryRoot};

/// Where review marks for identical file content are shared.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DiffReviewScope {
    Repository(RepositoryRoot),
    /// Every diff supplied as text outside a repository.
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiffFileReviewReference {
    pub scope: DiffReviewScope,
    pub path: RepositoryRelativePath,
    pub content_id: DiffReviewContentId,
}

/// Identity of a file's compared content and paths, independent of presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiffReviewContentId([u8; 32]);

impl DiffReviewContentId {
    #[must_use]
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    #[must_use]
    pub const fn into_digest(self) -> [u8; 32] {
        self.0
    }
}
