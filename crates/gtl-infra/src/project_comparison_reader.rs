use anyhow::Context as _;
use directories::BaseDirs;
use gtl_application::{ports::ProjectComparisonReader, projects::get_project_comparison_branch};
use gtl_models::{paths::RepositoryRoot, projects::comparison::ComparisonBranch};

use crate::app_state::SqliteAppState;

impl ProjectComparisonReader for SqliteAppState {
    fn comparison_branch(&self, path: &RepositoryRoot) -> anyhow::Result<Option<ComparisonBranch>> {
        let directories = BaseDirs::new().context("home directory is unavailable")?;
        let Ok(relative) = path.as_ref().strip_prefix(directories.home_dir()) else {
            return Ok(None);
        };
        let source = format!("~/{}", relative.to_string_lossy().replace('\\', "/"));
        let Ok(source) = gtl_models::projects::catalogue::ProjectDirectorySource::try_new(source)
        else {
            return Ok(None);
        };
        let connection = self.connection_lock()?;
        get_project_comparison_branch::execute(&source, &connection)
    }
}
