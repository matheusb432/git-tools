use std::path::Path;

use gtl_models::{paths::RepositoryRoot, projects::catalogue::ProjectId};
use rusqlite::{Connection, OptionalExtension as _};

pub struct FindProjectByRepository<'a> {
    pub path: &'a RepositoryRoot,
    pub home: &'a Path,
}

#[cqrsy::query]
pub fn execute(
    request: &FindProjectByRepository<'_>,
    connection: &Connection,
) -> anyhow::Result<Option<ProjectId>> {
    let Ok(relative) = request.path.as_ref().strip_prefix(request.home) else {
        return Ok(None);
    };
    let source = format!("~/{}", relative.to_string_lossy().replace('\\', "/"));
    let id: Option<String> = connection.query_row(
        "SELECT p.id FROM projects p JOIN project_sources s USING (source_id) WHERE s.source_kind = 'directory' AND s.source_value = ?1",
        [source], |row| row.get(0),
    ).optional()?;
    id.map(ProjectId::try_new).transpose().map_err(Into::into)
}
