//! Creates or restores each selected repository independently.

use std::path::Path;

use gtl_models::projects::catalogue::{
    ProjectDirectorySource, ProjectGroups, ProjectId, ProjectIds, ProjectMetadata,
    ProjectOperationMode, ProjectTitle,
};
use gtl_wire::viewer::projects::{
    ImportProjectRepositories, ProjectImportOutcome, ProjectImportResult, ProjectImportSelection,
};
use rusqlite::{Connection, OptionalExtension as _};

use crate::{
    projects::catalogue::{
        create_project::{self, CreateProject},
        set_project_membership::{self, ProjectMembership, SetProjectMembership},
    },
    repositories::find_repositories,
};

#[cqrsy::command]
pub fn execute(
    request: ImportProjectRepositories,
    connection: &mut Connection,
) -> Vec<ProjectImportResult> {
    request
        .selections
        .into_iter()
        .map(|selection| {
            let outcome = import_one(&selection, connection)
                .unwrap_or_else(|error| ProjectImportOutcome::Failed(error.to_string()));
            ProjectImportResult {
                path: selection.path,
                project_id: selection.project_id,
                outcome,
            }
        })
        .collect()
}

fn import_one(
    selection: &ProjectImportSelection,
    connection: &mut Connection,
) -> anyhow::Result<ProjectImportOutcome> {
    let source = ProjectDirectorySource::try_new(selection.path.clone())?;
    let path = Path::new(source.as_ref());
    anyhow::ensure!(
        path.canonicalize()? == path,
        "repository path changed; scan again"
    );
    anyhow::ensure!(
        find_repositories::is_discoverable_repository(path),
        "repository no longer has a discoverable .git marker"
    );

    let existing: Option<(String, bool, bool)> = connection
        .query_row(
            "SELECT p.id, p.paused_at IS NOT NULL, p.unmanaged_at IS NOT NULL
             FROM projects p JOIN project_sources s USING (source_id)
             WHERE s.source_kind = 'directory' AND s.source_value = ?1",
            [source.as_ref()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((existing_id, _, unmanaged)) = existing {
        anyhow::ensure!(
            selection.project_id == existing_id,
            "existing project ID changed; scan again"
        );
        anyhow::ensure!(unmanaged, "repository is already a managed project");
        let id: ProjectId = existing_id.try_into()?;
        let request = SetProjectMembership {
            ids: ProjectIds::try_new(vec![id])?,
            membership: ProjectMembership::Managed,
            mode: ProjectOperationMode::Apply,
        };
        set_project_membership::execute(&request, connection)?;
        return Ok(ProjectImportOutcome::Restored);
    }

    let id: ProjectId = selection.project_id.clone().try_into()?;
    let title: ProjectTitle = selection.title.clone().try_into()?;
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
    Ok(ProjectImportOutcome::Created)
}
