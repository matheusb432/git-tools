use gtl_models::projects::catalogue::{PROJECTS_MAX, ProjectId, ProjectMetadata};
use rusqlite::{Connection, TransactionBehavior, params};

use super::ProjectCatalogueError;
use crate::history::associate_render_projects;

pub struct CreateProject {
    pub id: ProjectId,
    pub metadata: ProjectMetadata,
    pub include_in_full_export: bool,
}

#[cqrsy::command]
pub fn execute(
    request: &CreateProject,
    connection: &mut Connection,
) -> Result<ProjectId, ProjectCatalogueError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let count: i64 =
        transaction.query_row("SELECT count(*) FROM projects", [], |row| row.get(0))?;
    if count >= i64::from(PROJECTS_MAX) {
        return Err(ProjectCatalogueError::LimitExceeded);
    }
    let source_id: i64 = transaction
        .query_one(
            "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', ?1)
             RETURNING source_id",
            [request.metadata.source.as_ref()],
            |row| row.get(0),
        )
        .map_err(create_error)?;
    transaction
        .execute(
            "INSERT INTO projects (id, source_id, title, git_remote, color, export_include_in_all)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                request.id.as_ref(),
                source_id,
                request.metadata.title.as_ref(),
                request
                    .metadata
                    .git_remote
                    .as_ref()
                    .map(AsRef::<str>::as_ref),
                request.metadata.color.as_ref().map(AsRef::<str>::as_ref),
                request.include_in_full_export
            ],
        )
        .map_err(create_error)?;
    for group in request.metadata.groups.as_slice() {
        transaction.execute(
            "INSERT INTO project_groups (project_id, group_name) VALUES (?1, ?2)",
            params![request.id.as_ref(), group.as_ref()],
        )?;
    }
    associate_render_projects::execute((), &transaction)
        .map_err(ProjectCatalogueError::InvalidData)?;
    transaction.commit()?;
    Ok(request.id.clone())
}

fn create_error(error: rusqlite::Error) -> ProjectCatalogueError {
    match error {
        rusqlite::Error::SqliteFailure(code, _)
            if matches!(
                code.extended_code,
                rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                    | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
            ) =>
        {
            ProjectCatalogueError::AlreadyExists
        }
        error => ProjectCatalogueError::Database(error),
    }
}
