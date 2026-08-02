use std::path::Path;

use anyhow::Context;
use gtl_application::ports::ConfiguredEditorClient;

mod git;
mod process;

#[derive(Debug, Clone, Copy, Default)]
pub struct GitConfiguredEditorClient;

impl ConfiguredEditorClient for GitConfiguredEditorClient {
    fn read_configured_command(&self, repository_root: &Path) -> anyhow::Result<String> {
        git::read_configured_command(repository_root)
    }

    fn launch(
        &self,
        program: &Path,
        arguments: &[String],
        working_directory: &Path,
    ) -> anyhow::Result<()> {
        let argument_refs = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        crate::detached_process::spawn_in(program, &argument_refs, working_directory)
            .with_context(|| format!("launch configured editor {}", program.display()))
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use gtl_application::ports::ConfiguredEditorClient;

    use super::GitConfiguredEditorClient;

    #[test]
    fn reads_the_repository_local_configured_editor_command() {
        let temporary = tempfile::tempdir().expect("temporary repository");
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(temporary.path())
                .status()
                .expect("git starts")
                .success()
        );
        assert!(
            Command::new("git")
                .args([
                    "config",
                    "core.editor",
                    r#"code --wait --profile "Work Tree""#,
                ])
                .current_dir(temporary.path())
                .status()
                .expect("git starts")
                .success()
        );

        assert_eq!(
            GitConfiguredEditorClient
                .read_configured_command(temporary.path())
                .expect("configured editor"),
            r#"code --wait --profile "Work Tree""#,
        );
    }
}
