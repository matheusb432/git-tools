use std::path::{Path, PathBuf};

use gtl_models::{
    failure::{Classification, Classified, ErrorClass, ExternalDiagnostic, ViewerFailure},
    paths::{AbsoluteFilePath, RepositoryRelativePath, RepositoryRoot},
};

use crate::{
    diffs::{FileStatus, View},
    ports::{
        FileSystemClient, FileSystemClientError, FileSystemClientErrorKind, FileSystemEntryKind,
        TextEditorClient,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum OpenDiffFileInConfiguredEditorError {
    #[error("the file is not present in the current diff")]
    FileNotInCurrentDiff,
    #[error("deleted diff files cannot be opened")]
    DiffFileDeleted,
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
    #[error("configured editor open failed: {0}")]
    ConfiguredEditorOpen(String),
}

impl Classified for OpenDiffFileInConfiguredEditorError {
    fn classify(&self) -> Classification {
        let failure = match self {
            Self::FileNotInCurrentDiff => ViewerFailure::FileNotInDiff,
            Self::DiffFileDeleted => ViewerFailure::FileDeleted,
            Self::DiffFileUnavailable => ViewerFailure::FileUnavailable,
            Self::DiffFileOutsideRepository => ViewerFailure::FileOutsideRepository,
            Self::FileSystem(_) => return Classification::Private(ErrorClass::Internal),
            Self::ConfiguredEditorDiscovery(detail)
            | Self::ConfiguredEditorCommand(detail)
            | Self::ConfiguredEditorOpen(detail) => ViewerFailure::EditorFailed {
                diagnostic: ExternalDiagnostic::new(detail),
            },
        };
        Classification::Public(failure.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextEditorInvocation {
    program: PathBuf,
    arguments: Vec<String>,
}

impl TextEditorInvocation {
    fn try_new(
        configured_command: &str,
        repository_root: &RepositoryRoot,
        file_path: &AbsoluteFilePath,
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
                repository_root.as_ref().to_string_lossy().into_owned(),
                "--goto".into(),
                file_path.as_path().to_string_lossy().into_owned(),
            ]);
        } else {
            words.push(file_path.as_path().to_string_lossy().into_owned());
        }

        Ok(Self {
            program,
            arguments: words,
        })
    }
}

#[cqrsy::command]
pub fn execute(
    diff_file_path: &RepositoryRelativePath,
    current_view: &View,
    file_system: &impl FileSystemClient,
    text_editor: &impl TextEditorClient,
) -> Result<(), OpenDiffFileInConfiguredEditorError> {
    let file = current_view
        .files
        .iter()
        .find(|file| &file.path == diff_file_path)
        .ok_or(OpenDiffFileInConfiguredEditorError::FileNotInCurrentDiff)?;
    if file.status() == FileStatus::Deleted {
        return Err(OpenDiffFileInConfiguredEditorError::DiffFileDeleted);
    }

    let repository_root = RepositoryRoot::try_new(canonicalize_required(
        file_system,
        current_view.repo_root.as_ref(),
    )?)
    .map_err(|error| OpenDiffFileInConfiguredEditorError::FileSystem(error.to_string()))?;
    let candidate_file_path = repository_root.join(diff_file_path);
    let file_path = AbsoluteFilePath::try_new(canonicalize_required(
        file_system,
        candidate_file_path.as_path(),
    )?)
    .map_err(|error| OpenDiffFileInConfiguredEditorError::FileSystem(error.to_string()))?;
    if !file_path.starts_with(repository_root.as_ref()) {
        return Err(OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository);
    }
    if file_system
        .entry_kind(file_path.as_path())
        .map_err(|error| map_file_system_error(&error))?
        != FileSystemEntryKind::File
    {
        return Err(OpenDiffFileInConfiguredEditorError::DiffFileUnavailable);
    }

    let configured_command = text_editor
        .read_command(&repository_root)
        .map_err(|error| {
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorDiscovery(error.to_string())
        })?;
    let invocation =
        TextEditorInvocation::try_new(&configured_command, &repository_root, &file_path)?;
    text_editor
        .open_in_file(&invocation.program, &invocation.arguments, &repository_root)
        .map_err(|error| {
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorOpen(error.to_string())
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

    use gtl_models::{
        diffs::DiffLineCount,
        paths::{RepositoryRelativePath, RepositoryRoot},
    };

    use super::{OpenDiffFileInConfiguredEditorError, TextEditorInvocation};
    use crate::{
        diffs::{FileDiff, FileStatus, View, open_diff_file_in_configured_editor},
        ports::{
            FileSystemClient, FileSystemClientError, FileSystemClientErrorKind,
            FileSystemEntryKind, TextEditorClient,
        },
        utils,
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
            self.canonicalize_results.lock().unwrap().len()
        }
    }

    impl FileSystemClient for ScriptedFileSystem {
        fn canonical_working_directory(&self) -> Result<PathBuf, FileSystemClientError> {
            self.canonicalize(Path::new("working-directory"))
        }

        fn canonicalize(&self, _: &Path) -> Result<PathBuf, FileSystemClientError> {
            self.canonicalize_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap()
        }

        fn entry_kind(&self, _: &Path) -> Result<FileSystemEntryKind, FileSystemClientError> {
            self.entry_kind_results.lock().unwrap().pop_front().unwrap()
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedOpen {
        program: PathBuf,
        arguments: Vec<String>,
        working_directory: PathBuf,
    }

    #[derive(Clone, Default)]
    struct ScriptedTextEditor {
        command_results: Arc<Mutex<VecDeque<anyhow::Result<String>>>>,
        open_result: Arc<Mutex<Option<anyhow::Result<()>>>>,
        opens: Arc<Mutex<Vec<RecordedOpen>>>,
    }

    impl ScriptedTextEditor {
        fn with_command(configured_command: anyhow::Result<String>) -> Self {
            Self {
                command_results: Arc::new(Mutex::new([configured_command].into_iter().collect())),
                ..Self::default()
            }
        }

        fn with_open_result(configured_command: String, open_result: anyhow::Result<()>) -> Self {
            Self {
                command_results: Arc::new(Mutex::new(
                    [Ok(configured_command)].into_iter().collect(),
                )),
                open_result: Arc::new(Mutex::new(Some(open_result))),
                ..Self::default()
            }
        }

        fn opened(&self) -> Option<RecordedOpen> {
            self.opens.lock().unwrap().first().cloned()
        }

        fn commands_remaining(&self) -> usize {
            self.command_results.lock().unwrap().len()
        }
    }

    impl TextEditorClient for ScriptedTextEditor {
        fn read_command(&self, _: &RepositoryRoot) -> anyhow::Result<String> {
            self.command_results.lock().unwrap().pop_front().unwrap()
        }

        fn open_in_file(
            &self,
            program: &Path,
            arguments: &[String],
            working_directory: &RepositoryRoot,
        ) -> anyhow::Result<()> {
            self.opens.lock().unwrap().push(RecordedOpen {
                program: program.to_path_buf(),
                arguments: arguments.to_vec(),
                working_directory: working_directory.as_ref().to_path_buf(),
            });
            self.open_result.lock().unwrap().take().unwrap_or(Ok(()))
        }
    }

    fn view<'path>(paths_and_statuses: impl IntoIterator<Item = (&'path str, FileStatus)>) -> View {
        View {
            file_filter: crate::diffs::file_filter::DiffFileFilter::default(),
            repo_root: utils::repository_root("/repos/git-tools"),
            files: paths_and_statuses
                .into_iter()
                .map(|(path, status)| FileDiff {
                    path: utils::repository_relative_path(path),
                    added: DiffLineCount::new(1),
                    removed: DiffLineCount::default(),
                    lines: match status {
                        FileStatus::Deleted => vec!["deleted file mode 100644".into()],
                        _ => Vec::new(),
                    }
                    .into(),
                    full_lines: None,
                })
                .collect(),
            ..utils::diffs::view()
        }
    }

    fn command(diff_file_path: impl AsRef<Path>) -> RepositoryRelativePath {
        RepositoryRelativePath::try_new(diff_file_path.as_ref().to_path_buf()).unwrap()
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
            let invocation = TextEditorInvocation::try_new(
                configured_command,
                &utils::repository_root("/repos/git-tools"),
                &utils::absolute_file_path("/repos/git-tools/src/main.rs"),
            )
            .unwrap();

            assert_eq!(invocation.program, PathBuf::from(expected_program));
            assert!(
                !invocation
                    .arguments
                    .iter()
                    .any(|argument| argument == "--wait" || argument == "-w")
            );
            assert!(invocation.arguments.ends_with(&[
                String::from("--reuse-window"),
                String::from("/repos/git-tools"),
                String::from("--goto"),
                String::from("/repos/git-tools/src/main.rs"),
            ]));
        }
    }

    #[test]
    fn current_modified_file_opens_in_the_configured_text_editor() {
        let file_system = available_file_system();
        let text_editor = ScriptedTextEditor::with_command(Ok("helix --reuse".into()));

        open_diff_file_in_configured_editor::execute(
            &command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &file_system,
            &text_editor,
        )
        .unwrap();

        assert_eq!(
            text_editor.opened(),
            Some(RecordedOpen {
                program: PathBuf::from("helix"),
                arguments: vec!["--reuse".into(), "/repos/git-tools/src/main.rs".into()],
                working_directory: PathBuf::from("/repos/git-tools"),
            })
        );
    }

    #[test]
    fn generic_editor_preserves_configured_arguments_and_appends_file() {
        let invocation = TextEditorInvocation::try_new(
            r#""/opt/IDE Suite/editor" --reuse"#,
            &utils::repository_root("/repos/git-tools"),
            &utils::absolute_file_path("/repos/git-tools/src/main.rs"),
        )
        .unwrap();

        assert_eq!(invocation.program, PathBuf::from("/opt/IDE Suite/editor"));
        assert_eq!(
            invocation.arguments,
            vec![
                String::from("--reuse"),
                String::from("/repos/git-tools/src/main.rs"),
            ]
        );
    }

    #[test]
    fn absent_deleted_or_changed_diff_entry_is_rejected_before_open() {
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
            let text_editor = ScriptedTextEditor::default();

            let error = open_diff_file_in_configured_editor::execute(
                &command(command_path),
                &current_view,
                &file_system,
                &text_editor,
            )
            .unwrap_err();

            assert_eq!(error.to_string(), expected_error.to_string());
            assert_eq!(file_system.canonicalize_results_remaining(), 1);
            assert_eq!(text_editor.opened(), None);
        }
    }

    #[test]
    fn absolute_parent_or_non_normal_path_is_rejected_by_the_request_type() {
        for path in ["/tmp/outside", "../outside", "src/./main.rs", ""] {
            assert!(RepositoryRelativePath::try_new(path.into()).is_err());
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
        let not_found_editor = ScriptedTextEditor::with_command(Ok("helix".into()));
        let not_found_error = open_diff_file_in_configured_editor::execute(
            &command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &not_found_file_system,
            &not_found_editor,
        )
        .unwrap_err();
        assert!(matches!(
            not_found_error,
            OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
        ));
        assert_eq!(not_found_editor.commands_remaining(), 1);
        assert_eq!(not_found_editor.opened(), None);

        let non_file_system = ScriptedFileSystem::with_results(
            [
                Ok(PathBuf::from("/repos/git-tools")),
                Ok(PathBuf::from("/repos/git-tools/src/main.rs")),
            ],
            [Ok(FileSystemEntryKind::Other)],
        );
        let non_file_editor = ScriptedTextEditor::with_command(Ok("helix".into()));
        let non_file_error = open_diff_file_in_configured_editor::execute(
            &command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &non_file_system,
            &non_file_editor,
        )
        .unwrap_err();
        assert!(matches!(
            non_file_error,
            OpenDiffFileInConfiguredEditorError::DiffFileUnavailable
        ));
        assert_eq!(non_file_editor.commands_remaining(), 1);
        assert_eq!(non_file_editor.opened(), None);

        let escaping_file_system = ScriptedFileSystem::with_canonicalize_results([
            Ok(PathBuf::from("/repos/git-tools")),
            Ok(PathBuf::from("/repos/outside.rs")),
        ]);
        let escaping_editor = ScriptedTextEditor::with_command(Ok("helix".into()));
        let escaping_error = open_diff_file_in_configured_editor::execute(
            &command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &escaping_file_system,
            &escaping_editor,
        )
        .unwrap_err();
        assert!(matches!(
            escaping_error,
            OpenDiffFileInConfiguredEditorError::DiffFileOutsideRepository
        ));
        assert_eq!(escaping_editor.commands_remaining(), 1);
        assert_eq!(escaping_editor.opened(), None);
    }

    #[test]
    fn empty_or_invalidly_quoted_configured_command_is_rejected_before_open() {
        for configured_command in ["", "   ", "\"unterminated"] {
            let text_editor = ScriptedTextEditor::with_command(Ok(configured_command.into()));

            let error = open_diff_file_in_configured_editor::execute(
                &command("src/main.rs"),
                &view([("src/main.rs", FileStatus::Modified)]),
                &available_file_system(),
                &text_editor,
            )
            .unwrap_err();

            assert!(matches!(
                error,
                OpenDiffFileInConfiguredEditorError::ConfiguredEditorCommand(_)
            ));
            assert_eq!(text_editor.opened(), None);
        }
    }

    #[test]
    fn file_system_editor_discovery_and_open_failures_remain_distinct() {
        let file_system_error = open_diff_file_in_configured_editor::execute(
            &command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &ScriptedFileSystem::with_canonicalize_results([Err(FileSystemClientError::new(
                FileSystemClientErrorKind::Other,
                "filesystem failure",
            ))]),
            &ScriptedTextEditor::default(),
        )
        .unwrap_err();
        assert!(matches!(
            file_system_error,
            OpenDiffFileInConfiguredEditorError::FileSystem(_)
        ));

        let discovery_error = open_diff_file_in_configured_editor::execute(
            &command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &available_file_system(),
            &ScriptedTextEditor::with_command(Err(anyhow::anyhow!("discovery failure"))),
        )
        .unwrap_err();
        assert!(matches!(
            discovery_error,
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorDiscovery(_)
        ));

        let open_error = open_diff_file_in_configured_editor::execute(
            &command("src/main.rs"),
            &view([("src/main.rs", FileStatus::Modified)]),
            &available_file_system(),
            &ScriptedTextEditor::with_open_result(
                "helix".into(),
                Err(anyhow::anyhow!("open failure")),
            ),
        )
        .unwrap_err();
        assert!(matches!(
            open_error,
            OpenDiffFileInConfiguredEditorError::ConfiguredEditorOpen(_)
        ));
    }
}
