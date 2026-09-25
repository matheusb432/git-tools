use anyhow::Context as _;
use gtl_models::{
    failure::{ErrorMeta, Failure, ProjectFailure, Resource},
    projects::catalogue::{Project, ProjectGroups, ProjectId, ProjectMetadata, ProjectStatus},
};
use rusqlite::{Connection, Row};

pub mod create_project;
pub mod get_project;
pub mod list_active_projects;
pub mod set_project_membership;

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ProjectCatalogueError {
    #[error("project was not found")]
    #[meta(failure = Failure::Gone { resource: Resource::Project })]
    NotFound,
    #[error("project ID, title, or source already exists")]
    #[meta(failure = ProjectFailure::AlreadyExists)]
    AlreadyExists,
    #[error("project catalogue exceeds its limit")]
    #[meta(failure = ProjectFailure::CatalogueFull { projects_max: u32::from(gtl_models::projects::catalogue::PROJECTS_MAX) })]
    LimitExceeded,
    #[error("project catalogue contains invalid data")]
    #[meta(private(DataLoss))]
    InvalidData(#[source] anyhow::Error),
    #[error("project database operation failed")]
    #[meta(private(Internal))]
    Database(#[from] rusqlite::Error),
}

const PROJECT_SELECT: &str = "SELECT p.id, p.title, s.source_kind, s.source_value,
    p.git_remote, p.color, p.paused_at
    FROM projects p JOIN project_sources s USING (source_id)";

fn read_project(row: &Row<'_>, connection: &Connection) -> Result<Project, ProjectCatalogueError> {
    let raw_id: String = row.get(0)?;
    let mut statement = connection.prepare_cached(
        "SELECT group_name FROM project_groups WHERE project_id = ?1 ORDER BY group_name LIMIT 65",
    )?;
    let raw_groups = statement
        .query_map([&raw_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let decode = || -> anyhow::Result<Project> {
        let id: ProjectId = raw_id.try_into()?;
        anyhow::ensure!(
            row.get::<_, String>(2)? == "directory",
            "invalid project source kind"
        );
        let groups = raw_groups
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Project {
            id,
            metadata: ProjectMetadata {
                title: row.get::<_, String>(1)?.try_into()?,
                source: row.get::<_, String>(3)?.try_into()?,
                git_remote: row
                    .get::<_, Option<String>>(4)?
                    .map(TryInto::try_into)
                    .transpose()?,
                color: row
                    .get::<_, Option<String>>(5)?
                    .map(TryInto::try_into)
                    .transpose()?,
                groups: ProjectGroups::try_new(groups).context("invalid project groups")?,
            },
            status: if row.get::<_, Option<String>>(6)?.is_some() {
                ProjectStatus::Paused
            } else {
                ProjectStatus::Active
            },
        })
    };
    decode().map_err(ProjectCatalogueError::InvalidData)
}
