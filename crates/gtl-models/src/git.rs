//! Role-safe values from Git configuration, references, and revision expressions.

use std::num::NonZeroU32;

use nutype::nutype;

use crate::diffs::{CommitId, CommitIdAbbreviation};

/// The checked-out state of a worktree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum GitHead {
    /// `HEAD` points to a local branch.
    Branch(BranchName),
    /// `HEAD` points directly to an object instead of a branch.
    Detached,
}

impl GitHead {
    /// Returns the checked-out branch, if `HEAD` is attached.
    #[must_use]
    pub const fn branch(&self) -> Option<&BranchName> {
        match self {
            Self::Branch(branch) => Some(branch),
            Self::Detached => None,
        }
    }
}

impl std::fmt::Display for GitHead {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Branch(branch) => branch.fmt(formatter),
            Self::Detached => formatter.write_str("HEAD"),
        }
    }
}

impl TryFrom<String> for GitHead {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value == "HEAD" {
            Ok(Self::Detached)
        } else {
            BranchName::try_new(value)
                .map(Self::Branch)
                .map_err(|error| error.to_string())
        }
    }
}

impl From<GitHead> for String {
    fn from(head: GitHead) -> Self {
        head.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHeadState {
    Unborn { branch: BranchName },
    Commit { head: GitHead, id: CommitId },
}

/// Selects whether a Git mutation is applied or only checked as a dry run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitEffectMode {
    Apply,
    DryRun,
}

impl GitEffectMode {
    #[must_use]
    pub const fn is_dry_run(self) -> bool {
        matches!(self, Self::DryRun)
    }
}

/// Counts commits in one semantic range or repository state.
#[nutype(
    const_fn,
    default = 0,
    derive(
        Debug,
        Clone,
        Copy,
        Default,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct CommitCount(u64);

/// Commit divergence relative to an upstream, named instead of tuple-positioned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct AheadBehind {
    pub behind: CommitCount,
    pub ahead: CommitCount,
}

/// A local branch name without a `refs/heads/` prefix.
#[nutype(
    validate(predicate = |value| !value.trim().is_empty()),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct BranchName(String);

impl BranchName {
    /// Returns the conventional default branch name.
    #[must_use]
    pub fn main() -> Self {
        known_valid(Self::try_new("main"))
    }
}

/// A tag name without a `refs/tags/` prefix.
#[nutype(
    validate(predicate = |value| !value.trim().is_empty()),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct TagName(String);

impl TagName {
    /// Builds the canonical `vN.N.N` tag for a stable semantic version.
    #[must_use]
    pub fn semantic_version(major: u64, minor: u64, patch: u64) -> Self {
        known_valid(Self::try_new(format!("v{major}.{minor}.{patch}")))
    }
}

/// The configured name of a Git remote.
#[nutype(
    validate(predicate = |value| !value.trim().is_empty()),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct RemoteName(String);

impl RemoteName {
    /// Returns the conventional primary remote name.
    #[must_use]
    pub fn origin() -> Self {
        known_valid(Self::try_new("origin"))
    }
}

/// A non-empty Git remote URL in any transport syntax supported by Git.
#[nutype(
    validate(predicate = |value| !value.trim().is_empty()),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct RemoteUrl(String);

/// A symbolic or concrete revision expression interpreted by Git.
#[nutype(
    validate(predicate = |value| !value.trim().is_empty()),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct GitRevision(String);

impl GitRevision {
    #[must_use]
    pub fn comparison_branch(branch: &crate::projects::comparison::ComparisonBranch) -> Self {
        known_valid(Self::try_new(format!("refs/heads/{branch}")))
    }

    /// Returns Git's symbolic current revision.
    #[must_use]
    pub fn head() -> Self {
        known_valid(Self::try_new("HEAD"))
    }

    /// Returns the configured upstream revision.
    #[must_use]
    pub fn upstream() -> Self {
        known_valid(Self::try_new("@{u}"))
    }

    /// Returns the ancestor `count` commits before `HEAD`.
    #[must_use]
    pub fn head_ancestor(count: NonZeroU32) -> Self {
        known_valid(Self::try_new(format!("HEAD~{count}")))
    }

    /// Returns the conventional default branch revision.
    #[must_use]
    pub fn main() -> Self {
        known_valid(Self::try_new("main"))
    }

    /// Returns the full local reference for `branch` as a revision expression.
    #[must_use]
    pub fn local_branch(branch: &BranchName) -> Self {
        known_valid(Self::try_new(format!("refs/heads/{branch}")))
    }

    /// Returns the previous reflog position of `branch`.
    #[must_use]
    pub fn previous_position(branch: &BranchName) -> Self {
        known_valid(Self::try_new(format!("{branch}@{{1}}")))
    }

    /// Returns the full remote-tracking reference for `branch`.
    #[must_use]
    pub fn remote_branch(remote: &RemoteName, branch: &BranchName) -> Self {
        known_valid(Self::try_new(format!("refs/remotes/{remote}/{branch}")))
    }

    /// Returns the abbreviated remote-tracking revision for `branch`.
    #[must_use]
    pub fn remote_tracking(remote: &RemoteName, branch: &BranchName) -> Self {
        known_valid(Self::try_new(format!("{remote}/{branch}")))
    }

    /// Returns a commit ID at a supported presentation width as a revision.
    #[must_use]
    pub fn abbreviated_commit(id: &CommitId, abbreviation: CommitIdAbbreviation) -> Self {
        known_valid(Self::try_new(id.abbreviated(abbreviation)))
    }
}

impl From<&CommitId> for GitRevision {
    fn from(id: &CommitId) -> Self {
        known_valid(Self::try_new(id.to_string()))
    }
}

impl From<&BranchName> for GitRevision {
    fn from(branch: &BranchName) -> Self {
        known_valid(Self::try_new(branch.to_string()))
    }
}

impl From<&TagName> for GitRevision {
    fn from(tag: &TagName) -> Self {
        known_valid(Self::try_new(tag.to_string()))
    }
}

impl From<&GitRefName> for GitRevision {
    fn from(reference: &GitRefName) -> Self {
        known_valid(Self::try_new(reference.to_string()))
    }
}

impl From<&GitRange> for GitRevision {
    fn from(range: &GitRange) -> Self {
        known_valid(Self::try_new(range.to_string()))
    }
}

/// A non-empty revision range interpreted by Git.
#[nutype(
    validate(predicate = |value| !value.trim().is_empty()),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct GitRange(String);

impl GitRange {
    /// Builds a two-dot range from two validated revisions.
    #[must_use]
    pub fn two_dot(base: &GitRevision, head: &GitRevision) -> Self {
        known_valid(Self::try_new(format!("{base}..{head}")))
    }

    /// Builds a three-dot range from two validated revisions.
    #[must_use]
    pub fn three_dot(left: &GitRevision, right: &GitRevision) -> Self {
        known_valid(Self::try_new(format!("{left}...{right}")))
    }

    /// Returns the commits on `HEAD` that are not in its configured upstream.
    #[must_use]
    pub fn upstream_to_head() -> Self {
        known_valid(Self::try_new("@{u}..HEAD"))
    }

    /// Returns the range containing only `commit`.
    #[must_use]
    pub fn single_commit(commit: &CommitId) -> Self {
        known_valid(Self::try_new(format!("{commit}^!")))
    }

    /// Returns the latest `count` commits ending at `HEAD`.
    #[must_use]
    pub fn head_commits(count: NonZeroU32) -> Self {
        known_valid(Self::try_new(format!("HEAD~{count}..HEAD")))
    }
}

/// The revision argument accepted by `git diff`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GitDiffSpec {
    /// Compare one revision with the index and working tree.
    AgainstWorkingTree(GitRevision),
    /// Compare the endpoints selected by a Git range expression.
    Range(GitRange),
}

impl GitDiffSpec {
    /// Returns the validated argument passed to Git.
    #[must_use]
    pub fn as_arg(&self) -> &str {
        match self {
            Self::AgainstWorkingTree(revision) => revision.as_ref(),
            Self::Range(range) => range.as_ref(),
        }
    }
}

impl std::fmt::Display for GitDiffSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_arg())
    }
}

/// A full or abbreviated Git reference name.
#[nutype(
    validate(predicate = |value| !value.trim().is_empty()),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct GitRefName(String);

impl GitRefName {
    /// Builds the full reference used to push one tag.
    #[must_use]
    pub fn for_tag(tag: &TagName) -> Self {
        known_valid(Self::try_new(format!("refs/tags/{tag}")))
    }
}

/// A canonical full Git object ID.
#[nutype(
    sanitize(with = normalize_object_id),
    validate(with = validate_object_id, error = GitObjectIdError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr,
        Serialize,
        Deserialize
    )
)]
pub struct GitObjectId(String);

impl From<&CommitId> for GitObjectId {
    fn from(id: &CommitId) -> Self {
        known_valid(Self::try_new(id.to_string()))
    }
}

/// Reports that raw input is not a supported full Git object hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Git object ID must contain 40 or 64 ASCII hexadecimal characters")]
pub struct GitObjectIdError;

fn normalize_object_id(mut value: String) -> String {
    value.make_ascii_lowercase();
    value
}

fn validate_object_id(value: &str) -> Result<(), GitObjectIdError> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(GitObjectIdError)
    }
}

#[allow(clippy::unreachable)]
fn known_valid<T, E>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(_) => unreachable!("a value assembled from validated parts remains valid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_git_roles_reject_only_empty_values() {
        assert!(BranchName::try_new(String::new()).is_err());
        assert!(TagName::try_new(String::new()).is_err());
        assert!(RemoteName::try_new(String::new()).is_err());
        assert!(RemoteUrl::try_new(String::new()).is_err());
        assert!(GitRevision::try_new(String::new()).is_err());
        assert!(GitRange::try_new(String::new()).is_err());
        assert!(GitRefName::try_new(String::new()).is_err());
        assert!(GitRevision::try_new("  ".to_owned()).is_err());

        let revision = GitRevision::try_new("HEAD^{tree}:path with spaces".to_owned()).unwrap();
        assert_eq!(revision.as_ref(), "HEAD^{tree}:path with spaces");
    }

    #[test]
    fn git_head_preserves_the_legacy_string_wire_shape() {
        let branch = GitHead::Branch(BranchName::try_new("feature").unwrap());

        assert_eq!(serde_json::to_string(&branch).unwrap(), r#""feature""#);
        assert_eq!(
            serde_json::from_str::<GitHead>(r#""feature""#).unwrap(),
            branch
        );
        assert_eq!(
            serde_json::to_string(&GitHead::Detached).unwrap(),
            r#""HEAD""#
        );
        assert_eq!(
            serde_json::from_str::<GitHead>(r#""HEAD""#).unwrap(),
            GitHead::Detached
        );
    }

    #[test]
    fn object_ids_accept_full_sha_one_and_sha_two_fifty_six_hashes() {
        let uppercase = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        let sha_one = GitObjectId::try_new(uppercase.to_owned()).unwrap();
        let sha_two_fifty_six = GitObjectId::try_new("a".repeat(64)).unwrap();

        assert_eq!(sha_one.as_ref(), "abcdef0123456789abcdef0123456789abcdef01");
        assert_eq!(sha_two_fifty_six.as_ref().len(), 64);
        assert!(GitObjectId::try_new("short".to_owned()).is_err());
    }

    #[test]
    fn serde_keeps_git_roles_as_strings_and_revalidates_input() {
        let branch = BranchName::try_new("feature/domain-types".to_owned()).unwrap();

        assert_eq!(
            serde_json::to_string(&branch).unwrap(),
            "\"feature/domain-types\""
        );
        assert!(serde_json::from_str::<BranchName>("\"\"").is_err());
    }
}
