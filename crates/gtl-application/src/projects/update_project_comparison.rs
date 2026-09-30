use gtl_models::{
    failure::{ErrorMeta, Failure},
    paths::ProjectName,
    projects::comparison::ComparisonBranch,
};
use gtl_wire::viewer::FieldUpdate;
use rusqlite::{Connection, params};

pub struct UpdateProjectComparison {
    pub project_name: ProjectName,
    pub comparison_branch: FieldUpdate<ComparisonBranch>,
    pub expected_comparison_branch: ComparisonBranch,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum UpdateProjectComparisonError {
    #[error("The project changed or is no longer available. Reload Projects and retry.")]
    #[meta(failure = Failure::Changed)]
    Conflict,
    #[error(transparent)]
    #[meta(private(Internal))]
    Database(#[from] rusqlite::Error),
}

#[cqrsy::command]
pub fn execute(
    request: UpdateProjectComparison,
    connection: &Connection,
) -> Result<(), UpdateProjectComparisonError> {
    let branch = match request.comparison_branch {
        FieldUpdate::Update(branch) => branch,
        FieldUpdate::Clear => ComparisonBranch::default(),
        FieldUpdate::Unchanged => return Ok(()),
    };
    let changed = connection.execute(
        "UPDATE projects SET comparison_branch = ?1 WHERE title = ?2 AND comparison_branch = ?3 AND unmanaged_at IS NULL",
        params![branch.as_ref(), request.project_name.as_str(), request.expected_comparison_branch.as_ref()],
    )?;
    if changed == 0 {
        return Err(UpdateProjectComparisonError::Conflict);
    }
    Ok(())
}
