//! Test fixtures for this crate's tests and, through the `testing` feature, downstream test
//! suites. Fixtures panic on setup failure so a test fails at the broken step.
#![allow(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "test fixtures report setup failures by panicking"
)]

use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(test)]
use gtl_models::diffs::{CommitId, PinnedRange};
use gtl_models::paths::RepositoryRoot;
use tempfile::TempDir;

#[cfg(test)]
pub(crate) fn commit_id(seed: &str) -> CommitId {
    seed.chars()
        .cycle()
        .take(40)
        .collect::<String>()
        .try_into()
        .unwrap()
}

#[cfg(test)]
pub(crate) fn pinned_range(base: &str, head: &str) -> PinnedRange {
    PinnedRange {
        base: commit_id(base),
        head: commit_id(head),
    }
}

/// A real Git repository on `main` for one test.
///
/// It has a commit identity and disabled signing, and its setup commands ignore the user's global
/// and system Git configuration.
#[derive(Debug)]
pub struct TestRepository {
    path: PathBuf,
    /// Keeps a temporary repository directory alive until the fixture drops.
    _directory: Option<TempDir>,
}

impl Default for TestRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRepository {
    /// Initializes an empty repository in a new temporary directory it removes on drop.
    #[must_use]
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let repository = Self::init(directory.path());
        Self {
            _directory: Some(directory),
            ..repository
        }
    }

    /// Initializes an empty repository at `path`, creating missing directories.
    #[must_use]
    pub fn init(path: impl Into<PathBuf>) -> Self {
        let repository = Self::open(path);
        std::fs::create_dir_all(&repository.path).unwrap();
        repository.git(&["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
        ] {
            repository.git(&["config", key, value]);
        }
        repository
    }

    /// Initializes an empty bare repository on `main` at `path`, creating missing directories.
    #[must_use]
    pub fn init_bare(path: impl Into<PathBuf>) -> Self {
        let repository = Self::open(path);
        std::fs::create_dir_all(&repository.path).unwrap();
        repository.git(&["init", "-q", "--bare", "-b", "main"]);
        repository
    }

    /// Wraps an existing repository, linked worktree, or submodule checkout without changing it.
    #[must_use]
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _directory: None,
        }
    }

    /// Initializes a bare repository at `path`, adds it as `origin`, and pushes `main` to it with
    /// upstream tracking.
    #[allow(
        clippy::return_self_not_must_use,
        reason = "tests that only push through `origin` discard the handle"
    )]
    pub fn add_bare_origin(&self, path: impl Into<PathBuf>) -> Self {
        let origin = Self::init_bare(path);
        self.git(&["remote", "add", "origin", origin.path.to_str().unwrap()]);
        self.git(&["push", "-q", "-u", "origin", "main"]);
        origin
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn root(&self) -> RepositoryRoot {
        RepositoryRoot::try_new(self.path.clone()).unwrap()
    }

    /// Runs Git in the repository and returns its trimmed standard output.
    pub fn git(&self, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(arguments)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?} failed in {}: {}",
            self.path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Writes `contents` to `relative_path`, creating parent directories.
    pub fn write(&self, relative_path: &str, contents: impl AsRef<[u8]>) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    /// Stages every change and commits it, even when nothing changed, returning the commit ID.
    pub fn commit_all(&self, message: &str) -> String {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
}
