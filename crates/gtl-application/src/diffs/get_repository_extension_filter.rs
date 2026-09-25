//! Reads the extension filter saved for one repository root in `gtl.db`.

use anyhow::Context as _;
use gtl_models::{
    diffs::{ExtensionFilter, FileExtensions},
    paths::RepositoryRoot,
};
use rusqlite::{Connection, OptionalExtension as _};

#[cqrsy::query]
pub fn execute(
    repository: &RepositoryRoot,
    connection: &Connection,
) -> anyhow::Result<ExtensionFilter> {
    // Saving rejects non-UTF-8 roots, so none can have a saved filter.
    let Some(repository_root) = repository.to_str() else {
        return Ok(ExtensionFilter::default());
    };
    let saved: Option<(String, String)> = connection
        .query_row(
            "SELECT mode, extensions_json FROM repository_extension_filters WHERE repository_root = ?1",
            [repository_root],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((mode, extensions_json)) = saved else {
        return Ok(ExtensionFilter::default());
    };
    let mode = mode
        .parse()
        .context("invalid extension filter mode in gtl.db")?;
    let extensions = serde_json::from_str::<Vec<String>>(&extensions_json)
        .context("invalid extension filter extensions in gtl.db")?;
    Ok(ExtensionFilter::new(mode, FileExtensions::new(extensions)))
}
