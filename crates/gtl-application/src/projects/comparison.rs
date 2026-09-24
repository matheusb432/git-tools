use gtl_models::{
    diffs::{CommitId, PinnedRange},
    failure::{Classification, Classified, ErrorClass, ProjectFailure},
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
        project: Option<RepositoryRoot>,
    },
    #[error("The repository at {path} has no commits to compare.")]
    Unborn { path: RepositoryRoot },
    #[error(
        "Local comparison branch '{branch}' and HEAD have no common ancestor in {path}. Change the project's comparison branch in Projects."
    )]
    NoCommonAncestor {
        path: RepositoryRoot,
        branch: ComparisonBranch,
        project: Option<RepositoryRoot>,
    },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

impl Classified for ComparisonError {
    fn classify(&self) -> Classification {
        let failure = match self {
            Self::MissingBranch {
                path,
                branch,
                project,
            } => ProjectFailure::ComparisonBranchMissing {
                path: path.clone(),
                branch: branch.clone(),
                project: project.clone(),
            },
            Self::Unborn { path } => ProjectFailure::RepositoryUnborn { path: path.clone() },
            Self::NoCommonAncestor {
                path,
                branch,
                project,
            } => ProjectFailure::NoCommonAncestor {
                path: path.clone(),
                branch: branch.clone(),
                project: project.clone(),
            },
            Self::Unexpected(_) => return Classification::Private(ErrorClass::Internal),
        };
        Classification::Public(failure.into())
    }
}

impl ComparisonError {
    #[must_use]
    pub const fn is_unavailable(&self) -> bool {
        !matches!(self, Self::Unexpected(_))
    }
}

/// The comparison branch that applies to a repository and the project whose setting names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredComparison {
    pub branch: ComparisonBranch,
    /// `None` when no catalogued project owns the repository, so the default applies.
    pub project: Option<RepositoryRoot>,
}

/// Reads the repository's own setting, then its primary worktree's, then the default.
pub fn configured_comparison(
    path: &RepositoryRoot,
    git: &impl GitClient,
    comparisons: &impl ProjectComparisonReader,
) -> anyhow::Result<ConfiguredComparison> {
    if let Some(branch) = comparisons.comparison_branch(path)? {
        return Ok(ConfiguredComparison {
            branch,
            project: Some(path.clone()),
        });
    }
    let primary = git.primary_worktree(path)?;
    if primary != *path
        && let Some(branch) = comparisons.comparison_branch(&primary)?
    {
        return Ok(ConfiguredComparison {
            branch,
            project: Some(primary),
        });
    }
    Ok(ConfiguredComparison {
        branch: ComparisonBranch::default(),
        project: None,
    })
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
    let ConfiguredComparison { branch, project } = configured_comparison(path, git, comparisons)?;
    let reference = branch.revision();
    if !git.revision_exists(path, &reference)? {
        return Err(ComparisonError::MissingBranch {
            path: path.clone(),
            branch,
            project,
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
            project,
        })?;
    Ok(ResolvedComparison::Branch {
        branch,
        tip,
        ancestor,
    })
}
