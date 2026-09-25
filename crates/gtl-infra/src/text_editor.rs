use std::path::Path;

use anyhow::Context;
use gtl_application::ports::TextEditorClient;
use gtl_models::paths::RepositoryRoot;

mod git;

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
    use crate::testing::TestRepository;

    const EDITOR_TEST_REPOSITORY: &str = "GTL_EDITOR_TEST_REPOSITORY";
    const EDITOR_TEST_HELPER: &str =
        "text_editor::tests::reads_repository_local_text_editor_command_in_subprocess";

    #[test]
    fn reads_the_repository_local_text_editor_command() {
        let repository = TestRepository::new();
        repository.git(&[
            "config",
            "core.editor",
            r#"code --wait --profile "Work Tree""#,
        ]);

        let output = Command::new(std::env::current_exe().unwrap())
            .args([EDITOR_TEST_HELPER, "--exact", "--ignored", "--nocapture"])
            .env(EDITOR_TEST_REPOSITORY, repository.path())
            .env_remove("GIT_EDITOR")
            .env_remove("VISUAL")
            .env_remove("EDITOR")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "editor test helper failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[test]
    #[ignore = "subprocess helper with isolated editor environment"]
    fn reads_repository_local_text_editor_command_in_subprocess() {
        let Some(repository) = std::env::var_os(EDITOR_TEST_REPOSITORY) else {
            return;
        };

        assert_eq!(
            GitTextEditorClient
                .read_command(
                    &gtl_models::paths::RepositoryRoot::try_new(repository.into()).unwrap(),
                )
                .unwrap(),
            r#"code --wait --profile "Work Tree""#,
        );
    }
}
