use gtl_models::projects::catalogue::{PROJECTS_MAX, ProjectId, ProjectMetadata};
use rusqlite::{Connection, TransactionBehavior, params};

use super::ProjectCatalogueError;

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
    transaction
        .execute(
            "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', ?1)",
            [request.metadata.source.as_ref()],
        )
        .map_err(create_error)?;
    let source_id = transaction.last_insert_rowid();
    transaction.execute(
        "INSERT INTO projects (id, source_id, title, git_remote, mux_session_name, affiliation, color, export_include_in_all)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![request.id.as_ref(), source_id, request.metadata.title.as_ref(), request.metadata.git_remote.as_ref().map(AsRef::<str>::as_ref),
            request.metadata.mux_session_name.as_ref(), request.metadata.affiliation.as_str(), request.metadata.color.as_ref().map(AsRef::<str>::as_ref), request.include_in_full_export],
    ).map_err(create_error)?;
    for group in request.metadata.groups.as_slice() {
        transaction.execute(
            "INSERT INTO project_groups (project_id, group_name) VALUES (?1, ?2)",
            params![request.id.as_ref(), group.as_ref()],
        )?;
    }
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
