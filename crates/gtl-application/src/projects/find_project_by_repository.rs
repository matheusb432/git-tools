use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    projects::catalogue::ProjectId,
};
use rusqlite::{Connection, OptionalExtension as _};

pub struct FindProjectByRepository<'a> {
    pub path: &'a RepositoryRoot,
}

#[cqrsy::query]
pub fn execute(
    request: &FindProjectByRepository<'_>,
    connection: &Connection,
) -> anyhow::Result<Option<ProjectId>> {
    let Some(source) = request.path.as_ref().to_str() else {
        return Ok(None);
    };
    let id: Option<String> = connection.query_row(
        "SELECT p.id FROM projects p JOIN project_sources s USING (source_id) WHERE s.source_kind = 'directory' AND s.source_value = ?1",
        [source], |row| row.get(0),
    ).optional()?;
    id.map(ProjectId::try_new).transpose().map_err(Into::into)
}

pub fn project_name(
    path: &RepositoryRoot,
    connection: &Connection,
) -> anyhow::Result<Option<ProjectName>> {
    let Some(source) = path.as_ref().to_str() else {
        return Ok(None);
    };
    let title: Option<String> = connection
        .query_row(
            "SELECT p.title FROM projects p JOIN project_sources s USING (source_id)
             WHERE s.source_kind = 'directory' AND s.source_value = ?1 AND p.unmanaged_at IS NULL",
            [source],
            |row| row.get(0),
        )
        .optional()?;
    title
        .map(ProjectName::try_new)
        .transpose()
        .map_err(Into::into)
}
