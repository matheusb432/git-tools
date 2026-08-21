use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::NamedTempFile;

use super::{
    BASE_SUBJECT, BASE_TIMESTAMP, DesktopScrollFixtureError, IDENTITY_EMAIL, IDENTITY_NAME,
    file_system_error,
};

const GIT_DIAGNOSTIC_BYTES_MAX: usize = 8 * 1024;
const GIT_ENVIRONMENT_VARIABLES_REMOVED: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_COUNT",
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_WORK_TREE",
];

pub(super) struct FixtureRepository {
    root: PathBuf,
    empty_git_config: NamedTempFile,
}

impl FixtureRepository {
    pub(super) fn initialize(root: &Path) -> Result<Self, DesktopScrollFixtureError> {
        fs::create_dir_all(root).map_err(file_system_error("create fixture repository", root))?;
        let empty_git_config = NamedTempFile::new()
            .map_err(file_system_error("create empty Git configuration", root))?;
        let repository = Self {
            root: root.to_path_buf(),
            empty_git_config,
        };
        repository.run("initialize repository", ["init", "-q", "-b", "main"])?;
        for (key, value) in [
            ("user.name", IDENTITY_NAME),
            ("user.email", IDENTITY_EMAIL),
            ("commit.gpgSign", "false"),
            ("core.autocrlf", "false"),
            ("core.fileMode", "false"),
            ("diff.renames", "true"),
        ] {
            repository.run("configure repository", ["config", key, value])?;
        }
        Ok(repository)
    }

    pub(super) fn initialize_existing(root: &Path) -> Result<Self, DesktopScrollFixtureError> {
        if !root.join(".git").is_dir() {
            return Err(super::invalid(format!(
                "hydrated fixture has no Git directory: {}",
                root.display()
            )));
        }
        let empty_git_config = NamedTempFile::new()
            .map_err(file_system_error("create empty Git configuration", root))?;
        Ok(Self {
            root: root.to_path_buf(),
            empty_git_config,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn commit_base(&self) -> Result<String, DesktopScrollFixtureError> {
        self.commit_all(
            BASE_SUBJECT,
            "Synthetic source state before the measured range.",
            BASE_TIMESTAMP,
        )
    }

    pub(super) fn commit_all(
        &self,
        subject: &str,
        body: &str,
        authored_at: &str,
    ) -> Result<String, DesktopScrollFixtureError> {
        self.run("stage fixture files", ["add", "-A"])?;
        self.output_with_identity(
            "commit fixture change",
            ["commit", "-q", "--no-gpg-sign", "-m", subject, "-m", body],
            authored_at,
        )?;
        self.output_text("read fixture commit ID", ["rev-parse", "HEAD"])
            .map(|value| value.trim().to_owned())
    }

    pub(super) fn format_head_patch(&self) -> Result<Vec<u8>, DesktopScrollFixtureError> {
        self.output(
            "format fixture patch",
            [
                "format-patch",
                "--stdout",
                "--no-signature",
                "--zero-commit",
                "--full-index",
                "--no-stat",
                "-1",
                "HEAD",
            ],
        )
    }

    pub(super) fn apply_patch(&self, patch: &Path) -> Result<(), DesktopScrollFixtureError> {
        let arguments = [
            OsString::from("am"),
            OsString::from("--committer-date-is-author-date"),
            OsString::from("--keep-cr"),
            OsString::from("--no-gpg-sign"),
            patch.as_os_str().to_owned(),
        ];
        self.output("apply fixture patch", arguments).map(|_| ())
    }

    pub(super) fn output_text<I, S>(
        &self,
        operation: &str,
        arguments: I,
    ) -> Result<String, DesktopScrollFixtureError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let bytes = self.output(operation, arguments)?;
        String::from_utf8(bytes).map_err(|error| {
            super::invalid(format!(
                "Git {operation} returned non-UTF-8 output at byte {}",
                error.utf8_error().valid_up_to()
            ))
        })
    }

    pub(super) fn run<I, S>(
        &self,
        operation: &str,
        arguments: I,
    ) -> Result<(), DesktopScrollFixtureError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.output(operation, arguments).map(|_| ())
    }

    fn output<I, S>(
        &self,
        operation: &str,
        arguments: I,
    ) -> Result<Vec<u8>, DesktopScrollFixtureError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command(arguments, None, operation)
    }

    fn output_with_identity<I, S>(
        &self,
        operation: &str,
        arguments: I,
        authored_at: &str,
    ) -> Result<Vec<u8>, DesktopScrollFixtureError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command(arguments, Some(authored_at), operation)
    }

    fn command<I, S>(
        &self,
        arguments: I,
        authored_at: Option<&str>,
        operation: &str,
    ) -> Result<Vec<u8>, DesktopScrollFixtureError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new("git");
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("GIT_CONFIG_GLOBAL", self.empty_git_config.path())
            .env("GIT_CONFIG_SYSTEM", self.empty_git_config.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("TZ", "UTC");
        for name in GIT_ENVIRONMENT_VARIABLES_REMOVED {
            command.env_remove(name);
        }
        if let Some(authored_at) = authored_at {
            command
                .env("GIT_AUTHOR_NAME", IDENTITY_NAME)
                .env("GIT_AUTHOR_EMAIL", IDENTITY_EMAIL)
                .env("GIT_AUTHOR_DATE", authored_at)
                .env("GIT_COMMITTER_NAME", IDENTITY_NAME)
                .env("GIT_COMMITTER_EMAIL", IDENTITY_EMAIL)
                .env("GIT_COMMITTER_DATE", authored_at);
        }
        let output = command.output().map_err(file_system_error(
            "start deterministic Git command",
            &self.root,
        ))?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        Err(DesktopScrollFixtureError::Git {
            operation: operation.to_owned(),
            exit_code: output.status.code().unwrap_or(-1),
            stderr: bounded_diagnostic(&output.stderr),
        })
    }
}

pub(super) fn copy_tree(
    source: &Path,
    destination: &Path,
) -> Result<(), DesktopScrollFixtureError> {
    fs::create_dir_all(destination).map_err(file_system_error(
        "create fixture tree destination",
        destination,
    ))?;
    let entries =
        fs::read_dir(source).map_err(file_system_error("read fixture tree source", source))?;
    for entry in entries {
        let entry = entry.map_err(file_system_error("read fixture tree entry", source))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(file_system_error(
            "read fixture tree entry type",
            &source_path,
        ))?;
        if file_type.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)
                .map_err(file_system_error("copy fixture tree file", &source_path))?;
        } else {
            return Err(super::invalid(format!(
                "fixture tree contains a non-file entry: {}",
                source_path.display()
            )));
        }
    }
    Ok(())
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    let end = bytes.len().min(GIT_DIAGNOSTIC_BYTES_MAX);
    let suffix = if bytes.len() > end {
        "...[truncated]"
    } else {
        ""
    };
    format!("{}{suffix}", String::from_utf8_lossy(&bytes[..end]).trim())
}
