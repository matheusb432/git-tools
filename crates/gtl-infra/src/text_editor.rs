use std::path::Path;

use anyhow::Context;
use gtl_application::ports::TextEditorClient;
use gtl_models::paths::RepositoryRoot;

mod git;
mod process;

#[derive(Debug, Clone, Copy, Default)]
pub struct GitTextEditorClient;

impl TextEditorClient for GitTextEditorClient {
    fn read_command(&self, repository_root: &RepositoryRoot) -> anyhow::Result<String> {
        git::read_command(repository_root.as_ref())
    }

    fn open_in_file(
        &self,
        program: &Path,
        arguments: &[String],
        working_directory: &RepositoryRoot,
    ) -> anyhow::Result<()> {
        let argument_refs = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        crate::detached_process::spawn_in(program, &argument_refs, working_directory.as_ref())
            .with_context(|| {
                format!(
                    "open file with configured text editor {}",
                    program.display()
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use gtl_application::ports::TextEditorClient;

    use super::GitTextEditorClient;

    #[test]
    fn reads_the_repository_local_text_editor_command() {
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
            GitTextEditorClient
                .read_command(
                    &gtl_models::paths::RepositoryRoot::try_new(temporary.path().to_path_buf(),)
                        .expect("temporary directory path is absolute"),
                )
                .expect("configured editor"),
            r#"code --wait --profile "Work Tree""#,
        );
    }
}
