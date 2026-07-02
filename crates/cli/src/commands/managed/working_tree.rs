//! Parsing `git status --porcelain`, shared by the `commit` and `status` fan-outs.

use std::path::Path;

use serde::Serialize;

use super::git_capture::git_capture;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommitFile {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DirtyState {
    pub(super) present: bool,
    pub(super) dirty: bool,
    pub(super) files: Vec<CommitFile>,
}

pub(super) fn dirty_state(repo: &Path) -> DirtyState {
    if !repo.join(".git").exists() {
        return DirtyState {
            present: false,
            dirty: false,
            files: Vec::new(),
        };
    }

    let output = match git_capture(repo, &["status", "--porcelain"]) {
        Ok(output) if output.success() => output.stdout,
        _ => String::new(),
    };
    let files = output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| CommitFile {
            status: line.get(0..2).unwrap_or("").trim().to_string(),
            path: line.get(3..).unwrap_or("").to_string(),
        })
        .collect::<Vec<_>>();

    DirtyState {
        present: true,
        dirty: !files.is_empty(),
        files,
    }
}
