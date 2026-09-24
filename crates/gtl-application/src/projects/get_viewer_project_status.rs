use gtl_models::{
    failure::{Classified as _, ProjectFailure, RepositoryFailure},
    paths::RepositoryRoot,
    projects::comparison::ComparisonBranch,
    repository::traversal::RepositoryTarget,
};
use gtl_wire::viewer::projects::{
    ViewerProject, ViewerProjectBranchComparison, ViewerProjectStatus,
};

use crate::{ports::GitClient, repositories::get_repository_statuses};

#[cqrsy::query]
pub fn execute(
    project: ViewerProject,
    git: &impl GitClient,
) -> anyhow::Result<ViewerProjectStatus> {
    let present = git.repo_present(&project.path);
    let path = if present {
        git.discover_top(project.path.as_ref())?
            .unwrap_or(project.path)
    } else {
        project.path
    };
    let status = get_repository_statuses::get_one(
        RepositoryTarget {
            label: project.name,
            path: path.clone(),
        },
        git,
    );
    let branch_comparison = if present {
        branch_comparison(&path, &project.comparison_branch, git)?
    } else {
        ViewerProjectBranchComparison::Unavailable {
            failure: RepositoryFailure::NotARepository {
                path: path.as_ref().to_path_buf(),
            }
            .into(),
        }
    };
    Ok(ViewerProjectStatus {
        project_id: project.id,
        status: status.repository().clone(),
        comparison_branch: project.comparison_branch,
        branch_comparison,
    })
}

/// Counts commits ahead of the comparison, reporting expected unavailability in band.
fn branch_comparison(
    path: &RepositoryRoot,
    comparison_branch: &ComparisonBranch,
    git: &impl GitClient,
) -> anyhow::Result<ViewerProjectBranchComparison> {
    let comparisons = std::collections::BTreeMap::from([(path.clone(), comparison_branch.clone())]);
    Ok(match super::comparison::resolve(path, git, &comparisons) {
        Ok(super::comparison::ResolvedComparison::Upstream { .. }) => {
            ViewerProjectBranchComparison::Upstream
        }
        Ok(comparison @ super::comparison::ResolvedComparison::Branch { .. }) => {
            match git.commit_count(path, &comparison.commit_range())? {
                Some(commits_ahead) => ViewerProjectBranchComparison::Branch { commits_ahead },
                None => ViewerProjectBranchComparison::Unavailable {
                    failure: ProjectFailure::CommitCountUnavailable.into(),
                },
            }
        }
        Err(error) if error.is_unavailable() => ViewerProjectBranchComparison::Unavailable {
            failure: error.classify().into_failure(),
        },
        Err(error) => return Err(error.into()),
    })
}
