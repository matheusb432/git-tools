//! Validated identities and metadata roles for stored diff artifacts.

use nutype::nutype;

use crate::diffs::{DiffKind, PinnedRange};

const STORE_HASH_CHARACTER_COUNT: usize = 16;

/// Stable repository identity used by the content-addressed artifact store.
#[nutype(
    sanitize(with = normalize_store_hash),
    validate(with = validate_repository_store_id, error = RepositoryStoreIdError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct RepositoryStoreId(String);

impl RepositoryStoreId {
    /// Builds the lowercase store ID represented by the first eight digest bytes.
    #[must_use]
    pub fn from_digest_prefix(prefix: [u8; 8]) -> Self {
        known_valid(Self::try_new(encode_digest_prefix(prefix)))
    }
}

/// Reports that a repository store ID is not the canonical short-hash shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("repository store ID must contain exactly 16 ASCII hexadecimal characters")]
pub struct RepositoryStoreIdError;

/// Content address of one exact rendered artifact.
#[nutype(
    sanitize(with = normalize_store_hash),
    validate(with = validate_artifact_content_hash, error = ArtifactContentHashError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct ArtifactContentHash(String);

impl ArtifactContentHash {
    /// Builds the lowercase content address represented by the first eight digest bytes.
    #[must_use]
    pub fn from_digest_prefix(prefix: [u8; 8]) -> Self {
        known_valid(Self::try_new(encode_digest_prefix(prefix)))
    }
}

/// Reports that an artifact content hash is not the canonical short-hash shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("artifact content hash must contain exactly 16 ASCII hexadecimal characters")]
pub struct ArtifactContentHashError;

/// A commit-range operator that can address an immutable stored artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactRangeKind {
    TwoDot,
    ThreeDot,
}

impl From<ArtifactRangeKind> for DiffKind {
    fn from(value: ArtifactRangeKind) -> Self {
        match value {
            ArtifactRangeKind::TwoDot => Self::TwoDot,
            ArtifactRangeKind::ThreeDot => Self::ThreeDot,
        }
    }
}

impl TryFrom<DiffKind> for ArtifactRangeKind {
    type Error = WorkTreeIsNotArtifactRange;

    fn try_from(value: DiffKind) -> Result<Self, Self::Error> {
        match value {
            DiffKind::TwoDot => Ok(Self::TwoDot),
            DiffKind::ThreeDot => Ok(Self::ThreeDot),
            DiffKind::WorkTree => Err(WorkTreeIsNotArtifactRange),
        }
    }
}

/// Reports an attempt to use mutable worktree state as an immutable range address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("worktree artifacts do not have an immutable commit-range address")]
pub struct WorkTreeIsNotArtifactRange;

/// Complete immutable address used by artifact range reuse.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactCommitRange {
    pub kind: ArtifactRangeKind,
    pub commits: PinnedRange,
}

/// The Git state represented by stored artifact metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArtifactDiffIdentity {
    WorkTree,
    CommitRange {
        kind: ArtifactRangeKind,
        /// Absent only when a rendered legacy or degraded artifact could not be pinned.
        commits: Option<PinnedRange>,
    },
}

impl ArtifactDiffIdentity {
    pub fn from_parts(
        kind: DiffKind,
        commits: Option<PinnedRange>,
    ) -> Result<Self, InvalidArtifactDiffIdentity> {
        match (kind, commits) {
            (DiffKind::WorkTree, None) => Ok(Self::WorkTree),
            (DiffKind::WorkTree, Some(_)) => Err(InvalidArtifactDiffIdentity),
            (kind, commits) => Ok(Self::CommitRange {
                kind: ArtifactRangeKind::try_from(kind).map_err(|_| InvalidArtifactDiffIdentity)?,
                commits,
            }),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> DiffKind {
        match self {
            Self::WorkTree => DiffKind::WorkTree,
            Self::CommitRange {
                kind: ArtifactRangeKind::TwoDot,
                ..
            } => DiffKind::TwoDot,
            Self::CommitRange {
                kind: ArtifactRangeKind::ThreeDot,
                ..
            } => DiffKind::ThreeDot,
        }
    }

    #[must_use]
    pub const fn commits(&self) -> Option<&PinnedRange> {
        match self {
            Self::WorkTree | Self::CommitRange { commits: None, .. } => None,
            Self::CommitRange {
                commits: Some(commits),
                ..
            } => Some(commits),
        }
    }

    #[must_use]
    pub fn matches_commit_range(&self, range: &ArtifactCommitRange) -> bool {
        matches!(
            self,
            Self::CommitRange {
                kind,
                commits: Some(commits),
            } if *kind == range.kind && commits == &range.commits
        )
    }
}

/// Reports incompatible artifact kind and commit-range metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("worktree artifact metadata cannot carry an immutable commit range")]
pub struct InvalidArtifactDiffIdentity;

fn normalize_store_hash(mut value: String) -> String {
    value.make_ascii_lowercase();
    value
}

fn validate_repository_store_id(value: &str) -> Result<(), RepositoryStoreIdError> {
    validate_store_hash(value)
        .then_some(())
        .ok_or(RepositoryStoreIdError)
}

fn validate_artifact_content_hash(value: &str) -> Result<(), ArtifactContentHashError> {
    validate_store_hash(value)
        .then_some(())
        .ok_or(ArtifactContentHashError)
}

fn validate_store_hash(value: &str) -> bool {
    value.len() == STORE_HASH_CHARACTER_COUNT && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn encode_digest_prefix(prefix: [u8; 8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(STORE_HASH_CHARACTER_COUNT);
    for byte in prefix {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[allow(clippy::unreachable)]
fn known_valid<T, E>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(_) => unreachable!("a digest prefix always produces a valid short hash"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diffs::CommitId;

    fn pinned_range() -> PinnedRange {
        PinnedRange {
            base: CommitId::try_from("a".repeat(40)).unwrap(),
            head: CommitId::try_from("b".repeat(40)).unwrap(),
        }
    }

    #[test]
    fn store_hash_roles_validate_and_normalize_the_same_wire_shape() {
        let repository = RepositoryStoreId::try_new("ABCDEF0123456789".to_owned()).unwrap();
        let content = ArtifactContentHash::try_new("0123456789ABCDEF".to_owned()).unwrap();

        assert_eq!(repository.as_ref(), "abcdef0123456789");
        assert_eq!(content.as_ref(), "0123456789abcdef");
        assert!(RepositoryStoreId::try_new("repository".to_owned()).is_err());
        assert!(ArtifactContentHash::try_new("short".to_owned()).is_err());
    }

    #[test]
    fn digest_prefixes_produce_distinct_role_safe_values() {
        let repository = RepositoryStoreId::from_digest_prefix([0xab; 8]);
        let content = ArtifactContentHash::from_digest_prefix([0xcd; 8]);

        assert_eq!(repository.as_ref(), "abababababababab");
        assert_eq!(content.as_ref(), "cdcdcdcdcdcdcdcd");
    }

    #[test]
    fn worktree_identity_cannot_carry_a_pinned_range() {
        assert_eq!(
            ArtifactDiffIdentity::from_parts(DiffKind::WorkTree, Some(pinned_range())),
            Err(InvalidArtifactDiffIdentity)
        );
        assert_eq!(
            ArtifactDiffIdentity::from_parts(DiffKind::WorkTree, None),
            Ok(ArtifactDiffIdentity::WorkTree)
        );
    }
}
