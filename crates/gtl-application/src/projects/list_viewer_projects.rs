use anyhow::Context as _;
use gtl_models::{
    projects::ProjectRepository,
    repository::{status::StatusClass, traversal::RepositoryTarget},
};
use gtl_wire::viewer::projects::{
    VIEWER_PROJECTS_MAX, ViewerProject, ViewerProjectBranchComparison,
};
use rusqlite::{Connection, OptionalExtension as _, params};

use crate::{ports::GitClient, repositories::get_repository_statuses};

#[cqrsy::query]
pub fn execute(
    repositories: Vec<ProjectRepository>,
    git: &impl GitClient,
    connection: &Connection,
) -> anyhow::Result<Vec<ViewerProject>> {
    anyhow::ensure!(
        repositories.len() <= VIEWER_PROJECTS_MAX,
        "project catalogue exceeds the viewer limit"
    );
    let mut projects = Vec::with_capacity(repositories.len());
    let mut statement = connection
        .prepare_cached("SELECT rendered_at FROM project_render_recency WHERE source_value = ?1")?;
    for repository in repositories {
        let path = if git.repo_present(&repository.path) {
            git.discover_top(repository.path.as_ref())
                .ok()
                .flatten()
                .unwrap_or(repository.path)
        } else {
            repository.path
        };
        let status = get_repository_statuses::get_one(
            RepositoryTarget {
                label: repository.name.clone(),
                path: path.clone(),
            },
            git,
        );
        let last_rendered_at = statement
            .query_row(params![path.to_string()], |row| row.get::<_, String>(0))
            .optional()?
            .map(TryInto::try_into)
            .transpose()
            .context("invalid project render timestamp")?;
        let raw_branch: Option<String> = connection
            .query_row(
                "SELECT comparison_branch FROM projects WHERE title = ?1",
                [repository.name.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let comparison_branch = raw_branch
            .map(gtl_models::projects::comparison::ComparisonBranch::try_new)
            .transpose()?
            .unwrap_or_default();
        let comparisons =
            std::collections::BTreeMap::from([(path.clone(), comparison_branch.clone())]);
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
        projects.push(ViewerProject {
            comparison_branch,
            branch_comparison,
            path,
            name: repository.name,
            status: status.repository().clone(),
            last_rendered_at,
        });
    }
    projects.sort_by(|left, right| {
        status_order(left.review_class())
            .cmp(&status_order(right.review_class()))
            .then_with(|| right.last_rendered_at.cmp(&left.last_rendered_at))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(projects)
}

fn status_order(status: StatusClass) -> u8 {
    match status {
        StatusClass::Pending => 0,
        StatusClass::Warn => 1,
        StatusClass::Clean => 2,
        StatusClass::Absent => 3,
    }
}
