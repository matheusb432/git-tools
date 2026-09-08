use gtl_models::{
    diffs::{CommitId, PinnedRange},
    git::{GitRange, GitRevision},
    paths::RepositoryRoot,
    projects::comparison::ComparisonBranch,
};

use crate::ports::{GitClient, GitEffect, ProjectComparisonReader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedComparison {
    Upstream {
        reference: GitRevision,
    },
    Branch {
        branch: ComparisonBranch,
        tip: CommitId,
        ancestor: CommitId,
    },
}

impl ResolvedComparison {
    #[must_use]
    pub fn reference(&self) -> GitRevision {
        match self {
            Self::Upstream { reference } => reference.clone(),
            Self::Branch { branch, .. } => branch.revision(),
        }
    }

    #[must_use]
    pub fn commit_range(&self) -> GitRange {
        GitRange::two_dot(&self.reference(), &GitRevision::head())
    }

    pub fn pin(&self, path: &RepositoryRoot, git: &impl GitClient) -> anyhow::Result<PinnedRange> {
        Ok(PinnedRange {
            base: match self {
                Self::Upstream { reference } => git.resolve_commit_id(path, reference)?,
                Self::Branch { ancestor, .. } => ancestor.clone(),
            },
            head: git.resolve_commit_id(path, &GitRevision::head())?,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ComparisonError {
    #[error(
        "Local comparison branch '{branch}' is missing in {path}. Change the project's comparison branch in Projects."
    )]
    MissingBranch {
        path: RepositoryRoot,
        branch: ComparisonBranch,
    },
    #[error("The repository at {path} has no commits to compare.")]
    Unborn { path: RepositoryRoot },
    #[error(
        "Local comparison branch '{branch}' and HEAD have no common ancestor in {path}. Change the project's comparison branch in Projects."
    )]
    NoCommonAncestor {
        path: RepositoryRoot,
        branch: ComparisonBranch,
    },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl ComparisonError {
    #[must_use]
    pub const fn is_unavailable(&self) -> bool {
        !matches!(self, Self::Unexpected(_))
    }
}

pub fn configured_branch(
    path: &RepositoryRoot,
    git: &impl GitClient,
    comparisons: &impl ProjectComparisonReader,
) -> anyhow::Result<ComparisonBranch> {
    if let Some(branch) = comparisons.comparison_branch(path)? {
        return Ok(branch);
    }
    let primary = git.primary_worktree(path)?;
    if primary != *path
        && let Some(branch) = comparisons.comparison_branch(&primary)?
    {
        return Ok(branch);
    }
    Ok(ComparisonBranch::default())
}

pub fn resolve(
    path: &RepositoryRoot,
    git: &impl GitClient,
    comparisons: &impl ProjectComparisonReader,
) -> Result<ResolvedComparison, ComparisonError> {
    if let GitEffect::Applied(reference) = git.upstream(path)? {
        return Ok(ResolvedComparison::Upstream {
            reference: GitRevision::from(&reference),
        });
    }
    resolve_local(path, git, comparisons)
}

pub(super) fn resolve_local(
    path: &RepositoryRoot,
    git: &impl GitClient,
    comparisons: &impl ProjectComparisonReader,
) -> Result<ResolvedComparison, ComparisonError> {
    let branch = configured_branch(path, git, comparisons)?;
    let reference = branch.revision();
    if !git.revision_exists(path, &reference)? {
        return Err(ComparisonError::MissingBranch {
            path: path.clone(),
            branch,
        });
    }
    if !git.revision_exists(path, &GitRevision::head())? {
        return Err(ComparisonError::Unborn { path: path.clone() });
    }
    let tip = git.resolve_commit_id(path, &reference)?;
    let ancestor = git
        .find_merge_base(path, &reference, &GitRevision::head())?
        .ok_or_else(|| ComparisonError::NoCommonAncestor {
            path: path.clone(),
            branch: branch.clone(),
        })?;
    Ok(ResolvedComparison::Branch {
        branch,
        tip,
        ancestor,
    })
}
