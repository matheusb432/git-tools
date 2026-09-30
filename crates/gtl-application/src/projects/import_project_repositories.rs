//! Creates or restores each selected repository independently.

use std::path::Path;

use gtl_models::{
    failure::{ErrorMeta, Failure, ProjectFailure, RepositoryFailure},
    paths::RepositoryRoot,
    projects::catalogue::{
        ProjectCollectionError, ProjectDirectorySource, ProjectGroups, ProjectId, ProjectIds,
        ProjectMetadata, ProjectOperationMode, ProjectTitle,
    },
};
use gtl_wire::viewer::projects::{ImportProjectRepositories, ProjectImportSelection};
use rusqlite::{Connection, OptionalExtension as _};

use crate::{
    projects::catalogue::{
        ProjectCatalogueError,
        create_project::{self, CreateProject},
        set_project_membership::{self, ProjectMembership, SetProjectMembership},
    },
    repositories::find_repositories,
};

/// How one selected repository joined the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportProjectRepositoryOk {
    Created,
    Restored,
}

/// Why one selected repository could not join the catalogue.
#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ImportProjectRepositoryError {
    /// The folder or its catalogue entry changed after the scan.
    #[error("repository changed since the scan")]
    #[meta(failure = ProjectFailure::ScanStale)]
    Stale,
    #[error("repository is already a managed project")]
    #[meta(failure = ProjectFailure::AlreadyManaged)]
    AlreadyManaged,
    /// The selection names an invalid field or a folder that is not a repository.
    #[error(transparent)]
    #[meta(failure)]
    Refused(Failure),
    #[error(transparent)]
    #[meta(transparent)]
    Catalogue(#[from] ProjectCatalogueError),
    #[error("inspect the selected repository")]
    #[meta(private(Internal))]
    Inspect(#[source] std::io::Error),
    #[error(transparent)]
    #[meta(private(Internal))]
    Collection(#[from] ProjectCollectionError),
}

/// The result of importing one selected repository.
#[derive(Debug)]
pub struct ImportProjectRepositoryAttempt {
    pub path: String,
    pub project_id: String,
    pub result: Result<ImportProjectRepositoryOk, ImportProjectRepositoryError>,
}

#[cqrsy::command]
pub fn execute(
    request: ImportProjectRepositories,
    connection: &mut Connection,
) -> Vec<ImportProjectRepositoryAttempt> {
    request
        .selections
        .into_iter()
        .map(|selection| ImportProjectRepositoryAttempt {
            result: import_one(&selection, connection),
            path: selection.path,
            project_id: selection.project_id,
        })
        .collect()
}

fn import_one(
    selection: &ProjectImportSelection,
    connection: &mut Connection,
) -> Result<ImportProjectRepositoryOk, ImportProjectRepositoryError> {
    let source = ProjectDirectorySource::try_new(selection.path.clone())
        .map_err(|_| invalid_selection("path"))?;
    let path = Path::new(source.as_ref());
    match path.canonicalize() {
        Ok(canonical) => {
            if !RepositoryRoot::try_new(canonical).is_ok_and(|canonical| canonical.as_ref() == path)
            {
                return Err(ImportProjectRepositoryError::Stale);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ImportProjectRepositoryError::Stale);
        }
        Err(error) => return Err(ImportProjectRepositoryError::Inspect(error)),
    }
    if !find_repositories::is_discoverable_repository(path) {
        return Err(ImportProjectRepositoryError::Refused(
            RepositoryFailure::NotARepository {
                path: path.to_path_buf(),
            }
            .into(),
        ));
    }

    let existing: Option<(String, bool, bool)> = connection
        .query_row(
            "SELECT p.id, p.paused_at IS NOT NULL, p.unmanaged_at IS NOT NULL
             FROM projects p JOIN project_sources s USING (source_id)
             WHERE s.source_kind = 'directory' AND s.source_value = ?1",
            [source.as_ref()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(ProjectCatalogueError::from)?;
    if let Some((existing_id, _, unmanaged)) = existing {
        if selection.project_id != existing_id {
            return Err(ImportProjectRepositoryError::Stale);
        }
        if !unmanaged {
            return Err(ImportProjectRepositoryError::AlreadyManaged);
        }
        let id: ProjectId = existing_id
            .try_into()
            .map_err(|error| ProjectCatalogueError::InvalidData(anyhow::Error::new(error)))?;
        let request = SetProjectMembership {
            ids: ProjectIds::try_new(vec![id])?,
            membership: ProjectMembership::Managed,
            mode: ProjectOperationMode::Apply,
        };
        set_project_membership::execute(&request, connection)?;
        return Ok(ImportProjectRepositoryOk::Restored);
    }

    let id: ProjectId = selection
        .project_id
        .clone()
        .try_into()
        .map_err(|_| invalid_selection("project_id"))?;
    let title: ProjectTitle = selection
        .title
        .clone()
        .try_into()
        .map_err(|_| invalid_selection("title"))?;
    let request = CreateProject {
        id,
        metadata: ProjectMetadata {
            title,
            source,
            git_remote: None,
            color: None,
            groups: ProjectGroups::try_new(Vec::new())?,
        },
        include_in_full_export: true,
    };
    create_project::execute(&request, connection)?;
    Ok(ImportProjectRepositoryOk::Created)
}

fn invalid_selection(field: &str) -> ImportProjectRepositoryError {
    ImportProjectRepositoryError::Refused(Failure::InvalidRequest {
        field: field.to_owned(),
    })
}
