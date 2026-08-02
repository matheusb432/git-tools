use std::path::Path;

pub trait ConfiguredEditorClient: Clone + Send + Sync + 'static {
    fn read_configured_command(&self, repository_root: &Path) -> anyhow::Result<String>;

    fn launch(
        &self,
        program: &Path,
        arguments: &[String],
        working_directory: &Path,
    ) -> anyhow::Result<()>;
}
