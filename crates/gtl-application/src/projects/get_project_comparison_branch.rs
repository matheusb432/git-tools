use anyhow::Context as _;
use gtl_models::projects::{catalogue::ProjectDirectorySource, comparison::ComparisonBranch};
use rusqlite::{Connection, OptionalExtension as _};

#[cqrsy::query]
pub fn execute(
    source: &ProjectDirectorySource,
    connection: &Connection,
) -> anyhow::Result<Option<ComparisonBranch>> {
    let raw: Option<String> = connection.query_row(
        "SELECT p.comparison_branch FROM projects p JOIN project_sources s USING (source_id) WHERE s.source_kind = 'directory' AND s.source_value = ?1",
        [source.as_ref()], |row| row.get(0),
    ).optional()?;
    raw.map(|value| {
        ComparisonBranch::try_new(value).context("invalid project comparison branch in gtl.db")
    })
    .transpose()
}
