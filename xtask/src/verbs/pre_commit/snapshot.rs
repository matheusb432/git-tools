use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use tempfile::{Builder, TempDir};

use crate::{process, task::Step, verbs::repository_root};

pub(super) struct StagedSnapshot {
    directory: TempDir,
    paths: Vec<PathBuf>,
}

impl StagedSnapshot {
    pub(super) fn create() -> Result<Option<Self>> {
        let paths_raw = process::capture_bytes(
            "staged paths",
            "git",
            &[
                "diff",
                "--cached",
                "--name-only",
                "--diff-filter=ACMR",
                "-z",
            ],
        )?;
        let paths = parse_paths(&paths_raw)?;
        if paths.is_empty() {
            return Ok(None);
        }

        let directory = create_snapshot_directory(&repository_root())?;
        let directory_text = directory
            .path()
            .to_str()
            .context("staged snapshot path is not UTF-8")?;
        let prefix = format!("{directory_text}{}", std::path::MAIN_SEPARATOR);
        process::run_step(&Step::new(
            "staged snapshot",
            "git",
            [
                "checkout-index".to_string(),
                "--all".to_string(),
                format!("--prefix={prefix}"),
            ],
        ))?;
        let paths = paths
            .into_iter()
            .filter(|path| {
                std::fs::symlink_metadata(directory.path().join(path))
                    .is_ok_and(|metadata| metadata.file_type().is_file())
            })
            .collect();

        Ok(Some(Self { directory, paths }))
    }

    pub(super) fn directory(&self) -> &Path {
        self.directory.path()
    }

    pub(super) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

/// Keep the snapshot beside the repository so workspace-relative sibling dependencies still
/// resolve from its copied manifest.
fn create_snapshot_directory(repository: &Path) -> Result<TempDir> {
    let repository = repository
        .canonicalize()
        .context("resolving repository root for a staged snapshot")?;
    let parent = repository
        .parent()
        .context("repository has no parent for a staged snapshot")?;
    Builder::new()
        .prefix(".git-tools-staged-")
        .tempdir_in(parent)
        .context("creating staged snapshot directory")
}

fn parse_paths(raw: &[u8]) -> Result<Vec<PathBuf>> {
    raw.split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let path = std::str::from_utf8(path).context("staged path is not UTF-8")?;
            let path = PathBuf::from(path);
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                bail!("staged path is not repository-relative: {}", path.display());
            }
            Ok(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_preserve_spaces_and_nul_boundaries() {
        assert_eq!(
            parse_paths(b"docs/Guide Name.md\0crates/gtl-models/src/lib.rs\0").unwrap(),
            [
                PathBuf::from("docs/Guide Name.md"),
                PathBuf::from("crates/gtl-models/src/lib.rs"),
            ]
        );
    }

    #[test]
    fn paths_reject_parent_traversal() {
        let error = parse_paths(b"../outside.rs\0").unwrap_err();
        assert!(error.to_string().contains("not repository-relative"));
    }

    #[test]
    fn paths_reject_non_utf8_bytes() {
        let error = parse_paths(b"invalid-\xff.rs\0").unwrap_err();
        assert!(error.to_string().contains("staged path is not UTF-8"));
    }

    #[test]
    fn snapshot_is_created_beside_the_repository() {
        let parent = tempfile::tempdir().unwrap();
        let repository = parent.path().join("git-tools");
        std::fs::create_dir(&repository).unwrap();

        let snapshot = create_snapshot_directory(&repository).unwrap();

        assert_eq!(snapshot.path().parent(), Some(parent.path()));
        assert!(
            snapshot
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".git-tools-staged-")
        );
    }
}
