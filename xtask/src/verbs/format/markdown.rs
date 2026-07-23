//! Markdown formatting plan (rumdl over tracked files).

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::FormatMode;
use crate::{process, task::Step};

/// `rumdl fmt [--check] <files>` over tracked Markdown, or `None` when the repo tracks none.
pub(super) fn format_step(mode: FormatMode) -> Result<Option<Step>> {
    let files = tracked_files()?;
    if files.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        Step::new("rumdl", "rumdl", ["fmt"])
            .with_arguments(mode.check_argument())
            .with_arguments(files),
    ))
}

pub(crate) fn check_step_for_files(files: Vec<PathBuf>, directory: &Path) -> Step {
    Step::new("rumdl", "rumdl", ["fmt", "--check"])
        .with_arguments(
            files
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned()),
        )
        .with_current_directory(directory)
}

/// Tracked Markdown files (NUL-split from `git ls-files`); gitignored paths are never returned.
fn tracked_files() -> Result<Vec<String>> {
    let raw = process::capture(
        "git ls-files",
        "git",
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.md",
        ],
    )?;
    Ok(raw
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}
