use std::path::Path;

use gtl_models::paths::RepositoryRoot;

pub trait TextEditorClient: Clone + Send + Sync + 'static {
    fn read_command(&self, repository_root: &RepositoryRoot) -> anyhow::Result<String>;

    fn open_in_file(
        &self,
        program: &Path,
        arguments: &[String],
        working_directory: &RepositoryRoot,
    ) -> anyhow::Result<()>;
}
