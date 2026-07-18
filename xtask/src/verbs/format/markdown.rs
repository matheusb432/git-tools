//! Markdown formatting plan (mdformat over tracked files).

use anyhow::Result;

use super::FormatMode;
use crate::{process, task::Step};

/// mdformat plugin set, kept in lockstep with `.mdformat.toml`.
const PLUGINS: &[&str] = &[
    "mdformat-gfm",
    "mdformat-gfm-alerts",
    "mdformat-wikilink",
    "mdformat-frontmatter",
];

/// `uvx … mdformat [--check] <files>` over tracked Markdown, or `None` when the repo tracks none.
pub(super) fn format_step(mode: FormatMode) -> Result<Option<Step>> {
    let files = tracked_files()?;
    if files.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        Step::new("mdformat", "uvx", ["--python", "3.13"])
            .with_arguments(PLUGINS.iter().flat_map(|plugin| ["--with", *plugin]))
            .with_arguments(["mdformat"])
            .with_arguments(mode.check_argument())
            .with_arguments(files),
    ))
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
