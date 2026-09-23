//! Annotates filesystem discovery results with the current project catalogue state.

use gtl_models::{projects::catalogue::PROJECTS_MAX, repository::traversal::RepositoryTarget};
use gtl_wire::viewer::projects::{
    DiscoveredProjectRepository, ProjectDiscovery, ProjectDiscoveryState,
};
use rusqlite::{Connection, OptionalExtension as _};

pub struct AnnotateProjectRepositories {
    pub root: String,
    pub repositories: Vec<RepositoryTarget>,
}

#[cqrsy::query]
pub fn execute(
    request: AnnotateProjectRepositories,
    connection: &Connection,
) -> anyhow::Result<ProjectDiscovery> {
    let AnnotateProjectRepositories { root, repositories } = request;
    anyhow::ensure!(
        repositories.len() <= usize::from(PROJECTS_MAX),
        "too many repositories were discovered"
    );
    let mut statement = connection.prepare_cached(
        "SELECT p.id, p.paused_at IS NOT NULL, p.unmanaged_at IS NOT NULL
         FROM projects p JOIN project_sources s USING (source_id)
         WHERE s.source_kind = 'directory' AND s.source_value = ?1",
    )?;
    let repositories = repositories
        .into_iter()
        .map(|repository| {
            let existing: Option<(String, bool, bool)> = statement
                .query_row([repository.path.to_string()], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .optional()?;
            let state = match existing {
                None => ProjectDiscoveryState::New,
                Some((id, paused, unmanaged)) => {
                    let id = id.try_into()?;
                    if unmanaged {
                        ProjectDiscoveryState::Unmanaged(id)
                    } else if paused {
                        ProjectDiscoveryState::Paused(id)
                    } else {
                        ProjectDiscoveryState::Active(id)
                    }
                }
            };
            Ok(DiscoveredProjectRepository {
                path: repository.path,
                label: repository.label,
                state,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(ProjectDiscovery { root, repositories })
}
