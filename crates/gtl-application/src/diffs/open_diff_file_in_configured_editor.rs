use std::path::{Component, Path, PathBuf};

use crate::{
    diffs::{FileStatus, View},
    ports::{
        ConfiguredEditorClient, FileSystemClient, FileSystemClientError, FileSystemClientErrorKind,
        FileSystemEntryKind,
    },
};

pub struct OpenDiffFileInConfiguredEditor {
    pub diff_file_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenDiffFileInConfiguredEditorError {
    #[error("the file is not present in the current diff")]
    FileNotInCurrentDiff,
    #[error("deleted diff files cannot be opened")]
    DiffFileDeleted,
    #[error("the diff file path is not a normalized relative path")]
    DiffFilePathInvalid,
    #[error("the diff file is unavailable")]
    DiffFileUnavailable,
    #[error("the diff file resolves outside the repository")]
    DiffFileOutsideRepository,
    #[error("filesystem access failed: {0}")]
    FileSystem(String),
    #[error("configured editor discovery failed: {0}")]
    ConfiguredEditorDiscovery(String),
    #[error("configured editor command is invalid: {0}")]
    ConfiguredEditorCommand(String),
    #[error("configured editor launch failed: {0}")]
    ConfiguredEditorLaunch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfiguredEditorLaunch {
    program: PathBuf,
    arguments: Vec<String>,
}

impl ConfiguredEditorLaunch {
    fn try_new(
        configured_command: &str,
        repository_root: &Path,
        file_path: &Path,
    ) -> Result<Self, OpenDiffFileInConfiguredEditorError> {
        let mut words = shlex::split(configured_command).ok_or_else(|| {
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorCommand(
                "command contains invalid quoting".into(),
            )
        })?;
        if words.is_empty() {
            return Err(
                OpenDiffFileInConfiguredEditorError::ConfiguredEditorCommand(
                    "command is empty".into(),
                ),
            );
        }

        let program = PathBuf::from(words.remove(0));
        let program_is_visual_studio_code_family = program
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_ascii_lowercase)
            .is_some_and(|stem| {
                matches!(
                    stem.as_str(),
                    "code" | "code-insiders" | "codium" | "vscodium"
                )
            });

        if program_is_visual_studio_code_family {
            words.retain(|argument| argument != "--wait" && argument != "-w");
            words.extend([
                "--reuse-window".into(),
                repository_root.to_string_lossy().into_owned(),
                "--goto".into(),
                file_path.to_string_lossy().into_owned(),
            ]);
        } else {
            words.push(file_path.to_string_lossy().into_owned());
        }

        Ok(Self {
            program,
            arguments: words,
        })
    }
}

#[cqrsy::command]
pub fn execute(
    command: OpenDiffFileInConfiguredEditor,
    current_view: &View,
    file_system: &impl FileSystemClient,
    configured_editor: &impl ConfiguredEditorClient,
) -> Result<(), OpenDiffFileInConfiguredEditorError> {
    let OpenDiffFileInConfiguredEditor { diff_file_path } = command;
    let relative_path = diff_file_path.as_path();
    let file = current_view
        .files
        .iter()
        .find(|file| Path::new(&file.path) == relative_path)
        .ok_or(OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff)?;
    if file.status() == FileStatus::Deleted {
        return Err(OpenDiffFileInConfiguredEditorError::DiffFileDeleted);
    }

    let path_is_normalized_relative = !relative_path.as_os_str().is_empty()
        && relative_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && relative_path
            .to_string_lossy()
            .split(['/', '\\'])
            .all(|component| !matches!(component, "" | "." | ".."));
    if !path_is_normalized_relative {
        return Err(OpenDiffFileInConfiguredEditorError::DiffFilePathInvalid);
    }

    let repository_root = canonicalize_required(file_system, Path::new(&current_view.repo_root))?;
    let file_path = canonicalize_required(file_system, &repository_root.join(relative_path))?;
    if !file_path.starts_with(&repository_root) {
        return Err(OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository);
    }
    if file_system
        .entry_kind(&file_path)
        .map_err(|error| map_file_system_error(&error))?
        != FileSystemEntryKind::File
    {
        return Err(OpenDiffFileInConfiguredEditorError::DiffFileUnavailable);
    }

    let configured_command = configured_editor
        .read_configured_command(&repository_root)
        .map_err(|error| {
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorDiscovery(error.to_string())
        })?;
    let launch =
        ConfiguredEditorLaunch::try_new(&configured_command, &repository_root, &file_path)?;
    configured_editor
        .launch(&launch.program, &launch.arguments, &repository_root)
        .map_err(|error| {
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorLaunch(error.to_string())
        })
}

fn canonicalize_required(
    file_system: &impl FileSystemClient,
    path: &Path,
) -> Result<PathBuf, OpenDiffFileInConfiguredEditorError> {
    file_system
        .canonicalize(path)
        .map_err(|error| map_file_system_error(&error))
}

fn map_file_system_error(error: &FileSystemClientError) -> OpenDiffFileInConfiguredEditorError {
    match error.kind() {
        FileSystemClientErrorKind::NotFound => {
            OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
        }
        FileSystemClientErrorKind::Other => {
            OpenDiffFileInConfiguredEditorError::FileSystem(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use super::{
        ConfiguredEditorLaunch, OpenDiffFileInConfiguredEditor,
        OpenDiffFileInConfiguredEditorError, execute,
    };
    use crate::{
        diffs::{FileDiff, FileStatus, View},
        ports::{
            ConfiguredEditorClient, FileSystemClient, FileSystemClientError,
            FileSystemClientErrorKind, FileSystemEntryKind,
        },
        testing,
    };

    #[derive(Clone, Default)]
    struct ScriptedFileSystem {
        canonicalize_results: Arc<Mutex<VecDeque<Result<PathBuf, FileSystemClientError>>>>,
        entry_kind_results:
            Arc<Mutex<VecDeque<Result<FileSystemEntryKind, FileSystemClientError>>>>,
    }

    impl ScriptedFileSystem {
        fn with_canonicalize_results(
            canonicalize_results: impl IntoIterator<Item = Result<PathBuf, FileSystemClientError>>,
        ) -> Self {
            Self {
                canonicalize_results: Arc::new(Mutex::new(
                    canonicalize_results.into_iter().collect(),
                )),
                ..Self::default()
            }
        }

        fn with_results(
            canonicalize_results: impl IntoIterator<Item = Result<PathBuf, FileSystemClientError>>,
            entry_kind_results: impl IntoIterator<
                Item = Result<FileSystemEntryKind, FileSystemClientError>,
            >,
        ) -> Self {
            Self {
                canonicalize_results: Arc::new(Mutex::new(
                    canonicalize_results.into_iter().collect(),
                )),
                entry_kind_results: Arc::new(Mutex::new(entry_kind_results.into_iter().collect())),
            }
        }

        fn canonicalize_results_remaining(&self) -> usize {
            self.canonicalize_results
                .lock()
                .expect("canonicalize results lock")
                .len()
        }
    }

    impl FileSystemClient for ScriptedFileSystem {
        fn canonical_working_directory(&self) -> Result<PathBuf, FileSystemClientError> {
            self.canonicalize(Path::new("working-directory"))
        }

        fn canonicalize(&self, _: &Path) -> Result<PathBuf, FileSystemClientError> {
            self.canonicalize_results
                .lock()
                .expect("canonicalize results lock")
                .pop_front()
                .expect("a scripted canonicalize result")
        }

        fn entry_kind(&self, _: &Path) -> Result<FileSystemEntryKind, FileSystemClientError> {
            self.entry_kind_results
                .lock()
                .expect("entry kind results lock")
                .pop_front()
                .expect("a scripted entry kind result")
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedLaunch {
        program: PathBuf,
        arguments: Vec<String>,
        working_directory: PathBuf,
    }

    #[derive(Clone, Default)]
    struct ScriptedConfiguredEditor {
        configured_command_results: Arc<Mutex<VecDeque<anyhow::Result<String>>>>,
        launch_result: Arc<Mutex<Option<anyhow::Result<()>>>>,
        launches: Arc<Mutex<Vec<RecordedLaunch>>>,
    }

    impl ScriptedConfiguredEditor {
        fn with_configured_command(configured_command: anyhow::Result<String>) -> Self {
            Self {
                configured_command_results: Arc::new(Mutex::new(
                    [configured_command].into_iter().collect(),
                )),
                ..Self::default()
            }
        }

        fn with_launch_result(
            configured_command: String,
            launch_result: anyhow::Result<()>,
        ) -> Self {
            Self {
                configured_command_results: Arc::new(Mutex::new(
                    [Ok(configured_command)].into_iter().collect(),
                )),
                launch_result: Arc::new(Mutex::new(Some(launch_result))),
                ..Self::default()
            }
        }

        fn launch(&self) -> Option<RecordedLaunch> {
            self.launches
                .lock()
                .expect("launches lock")
                .first()
                .cloned()
        }

        fn configured_commands_remaining(&self) -> usize {
            self.configured_command_results
                .lock()
                .expect("configured command results lock")
                .len()
        }
    }

    impl ConfiguredEditorClient for ScriptedConfiguredEditor {
        fn read_configured_command(&self, _: &Path) -> anyhow::Result<String> {
            self.configured_command_results
                .lock()
                .expect("configured command results lock")
                .pop_front()
                .expect("a scripted configured command result")
        }

        fn launch(
            &self,
            program: &Path,
            arguments: &[String],
            working_directory: &Path,
        ) -> anyhow::Result<()> {
            self.launches
                .lock()
                .expect("launches lock")
                .push(RecordedLaunch {
                    program: program.to_path_buf(),
                    arguments: arguments.to_vec(),
                    working_directory: working_directory.to_path_buf(),
                });
            self.launch_result
                .lock()
                .expect("launch result lock")
                .take()
                .unwrap_or(Ok(()))
        }
    }

    fn view<'path>(paths_and_statuses: impl IntoIterator<Item = (&'path str, FileStatus)>) -> View {
        View {
            repo_root: "/repos/git-tools".into(),
            files: paths_and_statuses
                .into_iter()
                .map(|(path, status)| FileDiff {
                    path: path.into(),
                    added: 1,
                    removed: 0,
                    lines: match status {
                        FileStatus::Deleted => vec!["deleted file mode 100644".into()],
                        _ => Vec::new(),
                    },
                    full_lines: None,
                })
                .collect(),
            ..testing::diffs::view()
        }
    }

    fn command(diff_file_path: impl Into<PathBuf>) -> OpenDiffFileInConfiguredEditor {
        OpenDiffFileInConfiguredEditor {
            diff_file_path: diff_file_path.into(),
        }
    }

    fn available_file_system() -> ScriptedFileSystem {
        ScriptedFileSystem::with_results(
            [
                Ok(PathBuf::from("/repos/git-tools")),
                Ok(PathBuf::from("/repos/git-tools/src/main.rs")),
            ],
            [Ok(FileSystemEntryKind::File)],
        )
    }

    #[test]
    fn visual_studio_code_family_opens_repository_and_focuses_file_without_waiting() {
        for (configured_command, expected_program) in [
            ("code --wait --profile Work", "code"),
            ("code-insiders -w", "code-insiders"),
            ("codium.cmd --wait", "codium.cmd"),
            (
                r#""C:/Program Files/VSCodium/vscodium.exe" -w"#,
                "C:/Program Files/VSCodium/vscodium.exe",
            ),
        ] {
            let launch = ConfiguredEditorLaunch::try_new(
                configured_command,
                Path::new("/repos/git-tools"),
                Path::new("/repos/git-tools/src/main.rs"),
            )
            .expect("configured command parses");

            assert_eq!(launch.program, PathBuf::from(expected_program));
            assert!(
                !launch
                    .arguments
                    .iter()
                    .any(|argument| argument == "--wait" || argument == "-w")
            );
            assert!(launch.arguments.ends_with(&[
                String::from("--reuse-window"),
                String::from("/repos/git-tools"),
                String::from("--goto"),
                String::from("/repos/git-tools/src/main.rs"),
            ]));
        }
    }

    #[test]
    fn current_modified_file_launches_the_configured_editor() {
        let file_system = available_file_system();
        let configured_editor =
            ScriptedConfiguredEditor::with_configured_command(Ok("helix --reuse".into()));

        execute(
            command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &file_system,
            &configured_editor,
        )
        .expect("configured editor launches");

        assert_eq!(
            configured_editor.launch(),
            Some(RecordedLaunch {
                program: PathBuf::from("helix"),
                arguments: vec!["--reuse".into(), "/repos/git-tools/src/main.rs".into()],
                working_directory: PathBuf::from("/repos/git-tools"),
            })
        );
    }

    #[test]
    fn generic_editor_preserves_configured_arguments_and_appends_file() {
        let launch = ConfiguredEditorLaunch::try_new(
            r#""/opt/IDE Suite/editor" --reuse"#,
            Path::new("/repos/git-tools"),
            Path::new("/repos/git-tools/src/main.rs"),
        )
        .expect("configured command parses");

        assert_eq!(launch.program, PathBuf::from("/opt/IDE Suite/editor"));
        assert_eq!(
            launch.arguments,
            vec![
                String::from("--reuse"),
                String::from("/repos/git-tools/src/main.rs"),
            ]
        );
    }

    #[test]
    fn absent_deleted_or_changed_diff_entry_is_rejected_before_launch() {
        for (command_path, current_view, expected_error) in [
            (
                PathBuf::from("src/missing.rs"),
                view([("src/main.rs", FileStatus::Modified)]),
                OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff,
            ),
            (
                PathBuf::from("src/main.rs"),
                view([("src/main.rs", FileStatus::Deleted)]),
                OpenDiffFileInConfiguredEditorError::DiffFileDeleted,
            ),
            (
                PathBuf::from("src/main.rs"),
                view([("src/renamed.rs", FileStatus::Modified)]),
                OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff,
            ),
        ] {
            let file_system =
                ScriptedFileSystem::with_canonicalize_results([Ok(PathBuf::from("unused"))]);
            let configured_editor = ScriptedConfiguredEditor::default();

            let error = execute(
                command(command_path),
                &current_view,
                &file_system,
                &configured_editor,
            )
            .expect_err("invalid diff entry is rejected");

            assert_eq!(error.to_string(), expected_error.to_string());
            assert_eq!(file_system.canonicalize_results_remaining(), 1);
            assert_eq!(configured_editor.launch(), None);
        }
    }

    #[test]
    fn absolute_parent_or_non_normal_path_is_rejected_before_file_system_access() {
        for path in ["/tmp/outside", "../outside", "src/./main.rs", ""] {
            let file_system =
                ScriptedFileSystem::with_canonicalize_results([Ok(PathBuf::from("unused"))]);

            let error = execute(
                command(path),
                &view([(path, FileStatus::Modified)]),
                &file_system,
                &ScriptedConfiguredEditor::default(),
            )
            .expect_err("non-normal diff path is rejected");

            assert!(matches!(
                error,
                OpenDiffFileInConfiguredEditorError::DiffFilePathInvalid
            ));
            assert_eq!(file_system.canonicalize_results_remaining(), 1);
        }
    }

    #[test]
    fn missing_non_file_or_canonically_escaping_path_is_rejected_before_editor_discovery() {
        let not_found_file_system = ScriptedFileSystem::with_canonicalize_results([
            Ok(PathBuf::from("/repos/git-tools")),
            Err(FileSystemClientError::new(
                FileSystemClientErrorKind::NotFound,
                "missing",
            )),
        ]);
        let not_found_editor =
            ScriptedConfiguredEditor::with_configured_command(Ok("helix".into()));
        let not_found_error = execute(
            command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &not_found_file_system,
            &not_found_editor,
        )
        .expect_err("missing file is unavailable");
        assert!(matches!(
            not_found_error,
            OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
        ));
        assert_eq!(not_found_editor.configured_commands_remaining(), 1);
        assert_eq!(not_found_editor.launch(), None);

        let non_file_system = ScriptedFileSystem::with_results(
            [
                Ok(PathBuf::from("/repos/git-tools")),
                Ok(PathBuf::from("/repos/git-tools/src/main.rs")),
            ],
            [Ok(FileSystemEntryKind::Other)],
        );
        let non_file_editor = ScriptedConfiguredEditor::with_configured_command(Ok("helix".into()));
        let non_file_error = execute(
            command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &non_file_system,
            &non_file_editor,
        )
        .expect_err("non-file is unavailable");
        assert!(matches!(
            non_file_error,
            OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
        ));
        assert_eq!(non_file_editor.configured_commands_remaining(), 1);
        assert_eq!(non_file_editor.launch(), None);

        let escaping_file_system = ScriptedFileSystem::with_canonicalize_results([
            Ok(PathBuf::from("/repos/git-tools")),
            Ok(PathBuf::from("/repos/outside.rs")),
        ]);
        let escaping_editor = ScriptedConfiguredEditor::with_configured_command(Ok("helix".into()));
        let escaping_error = execute(
            command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &escaping_file_system,
            &escaping_editor,
        )
        .expect_err("escaping file is rejected");
        assert!(matches!(
            escaping_error,
            OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository
        ));
        assert_eq!(escaping_editor.configured_commands_remaining(), 1);
        assert_eq!(escaping_editor.launch(), None);
    }

    #[test]
    fn empty_or_invalidly_quoted_configured_command_is_rejected_before_launch() {
        for configured_command in ["", "   ", "\"unterminated"] {
            let configured_editor =
                ScriptedConfiguredEditor::with_configured_command(Ok(configured_command.into()));

            let error = execute(
                command("src/main.rs"),
                &view([("src/main.rs", FileStatus::Modified)]),
                &available_file_system(),
                &configured_editor,
            )
            .expect_err("invalid configured command is rejected");

            assert!(matches!(
                error,
                OpenDiffFileInConfiguredEditorError::ConfiguredEditorCommand(_)
            ));
            assert_eq!(configured_editor.launch(), None);
        }
    }

    #[test]
    fn file_system_editor_discovery_and_launch_failures_remain_distinct() {
        let file_system_error = execute(
            command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &ScriptedFileSystem::with_canonicalize_results([Err(FileSystemClientError::new(
                FileSystemClientErrorKind::Other,
                "filesystem failure",
            ))]),
            &ScriptedConfiguredEditor::default(),
        )
        .expect_err("filesystem failure is surfaced");
        assert!(matches!(
            file_system_error,
            OpenDiffFileInConfiguredEditorError::FileSystem(_)
        ));

        let discovery_error = execute(
            command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &available_file_system(),
            &ScriptedConfiguredEditor::with_configured_command(Err(anyhow::anyhow!(
                "discovery failure"
            ))),
        )
        .expect_err("configured editor discovery failure is surfaced");
        assert!(matches!(
            discovery_error,
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorDiscovery(_)
        ));

        let launch_error = execute(
            command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &available_file_system(),
            &ScriptedConfiguredEditor::with_launch_result(
                "helix".into(),
                Err(anyhow::anyhow!("launch failure")),
            ),
        )
        .expect_err("configured editor launch failure is surfaced");
        assert!(matches!(
            launch_error,
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorLaunch(_)
        ));
    }
}
