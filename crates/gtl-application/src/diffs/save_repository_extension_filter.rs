//! Replaces the extension filter saved for one repository root in `gtl.db`.

use anyhow::Context as _;
use gtl_models::{diffs::ExtensionFilter, paths::RepositoryRoot};
use rusqlite::{Connection, params};

pub struct SaveRepositoryExtensionFilter<'a> {
    pub repository: &'a RepositoryRoot,
    /// An inactive filter deletes the saved row.
    pub filter: &'a ExtensionFilter,
}

#[cqrsy::command]
pub fn execute(
    request: &SaveRepositoryExtensionFilter<'_>,
    connection: &Connection,
) -> anyhow::Result<()> {
    let repository_root = request
        .repository
        .to_str()
        .context("cannot save an extension filter for a non-UTF-8 repository path")?;
    if !request.filter.is_active() {
        connection.execute(
            "DELETE FROM repository_extension_filters WHERE repository_root = ?1",
            [repository_root],
        )?;
        return Ok(());
    }
    let extensions_json = serde_json::to_string(request.filter.extensions())?;
    connection.execute(
        "INSERT INTO repository_extension_filters (repository_root, mode, extensions_json) VALUES (?1, ?2, ?3)
         ON CONFLICT (repository_root) DO UPDATE SET mode = excluded.mode, extensions_json = excluded.extensions_json",
        params![repository_root, request.filter.mode().as_str(), extensions_json],
    )?;
    Ok(())
}
