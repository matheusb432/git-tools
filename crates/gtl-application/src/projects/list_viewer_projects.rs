use anyhow::Context as _;
use gtl_models::{
    projects::ProjectRepository,
    repository::{status::StatusClass, traversal::RepositoryTarget},
};
use gtl_wire::viewer::projects::{VIEWER_PROJECTS_MAX, ViewerProject};
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
        projects.push(ViewerProject {
            path,
            name: repository.name,
            status: status.repository().clone(),
            last_rendered_at,
        });
    }
    projects.sort_by(|left, right| {
        status_order(left.status.class())
            .cmp(&status_order(right.status.class()))
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
