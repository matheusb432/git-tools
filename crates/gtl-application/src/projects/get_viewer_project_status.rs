use gtl_models::repository::traversal::RepositoryTarget;
use gtl_wire::viewer::projects::{
    ViewerProject, ViewerProjectBranchComparison, ViewerProjectStatus,
};

use crate::{ports::GitClient, repositories::get_repository_statuses};

#[cqrsy::query]
pub fn execute(
    project: ViewerProject,
    git: &impl GitClient,
) -> anyhow::Result<ViewerProjectStatus> {
    let path = if git.repo_present(&project.path) {
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
    let comparisons =
        std::collections::BTreeMap::from([(path.clone(), project.comparison_branch.clone())]);
    let branch_comparison = match super::comparison::resolve(&path, git, &comparisons) {
        Ok(super::comparison::ResolvedComparison::Upstream { .. }) => {
            ViewerProjectBranchComparison::Upstream
        }
        Ok(comparison @ super::comparison::ResolvedComparison::Branch { .. }) => {
            match git.commit_count(&path, &comparison.commit_range())? {
                Some(commits_ahead) => ViewerProjectBranchComparison::Branch { commits_ahead },
                None => ViewerProjectBranchComparison::Unavailable {
                    reason: "Commit count is unavailable".to_owned(),
                },
            }
        }
        Err(error) => ViewerProjectBranchComparison::Unavailable {
            reason: error.to_string(),
        },
    };
    Ok(ViewerProjectStatus {
        project_id: project.id,
        status: status.repository().clone(),
        comparison_branch: project.comparison_branch,
        branch_comparison,
    })
}
