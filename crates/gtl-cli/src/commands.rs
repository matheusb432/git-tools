use std::path::{Path, PathBuf};

use anyhow::Context as _;

use crate::{commands::diff::DiffOutcome, server_client::ServerClient};

pub mod diff;
pub mod diff_live;
pub mod diff_subrepos;
pub mod managed;
pub mod merge_diff;
pub mod prune;
pub mod push_subrepos;
pub mod server_ctl;
pub mod sync;
pub mod tag;
pub mod worktree;

pub(crate) fn canonical_working_directory() -> anyhow::Result<PathBuf> {
    let current = std::env::current_dir().context("resolving the current directory")?;
    std::fs::canonicalize(&current)
        .with_context(|| format!("canonicalizing current directory {}", current.display()))
}

/// Forward one recipe batch to the single-instance viewer as one argv token.
pub(crate) fn forward_recipes(batch: &gtl_wire::recipes::OpenRecipes) -> anyhow::Result<()> {
    use crate::viewer::{no_open_requested, resolve_viewer_bin};

    if no_open_requested() {
        return Ok(());
    }
    let bin = resolve_viewer_bin()
        .context("gtl-viewer is not installed; cannot forward the recipe batch")?;
    let token = gtl_wire::recipes::encode_token(batch)
        .context("failed to encode the viewer recipe batch")?;
    crate::detached_process::spawn(&bin, &[token.as_str()])
        .context("failed to spawn gtl-viewer to forward the recipe batch")
}

/// Renders `path` as a `file://` URL for the terminal. Not full RFC 8089
/// percent-encoding — store artifact paths are built from repo names/content
/// hashes, never arbitrary user input — just forward-slash normalization so a
/// Windows-style `C:\...` path (Git Bash) still yields a well-formed URL.
pub(crate) fn file_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix('/') {
        format!("file:///{rest}")
    } else {
        format!("file:///{normalized}")
    }
}

pub(crate) fn present<PrepareOperationResponse, RenderOperationResponse>(
    raw: bool,
    prepare: impl FnOnce(&ServerClient) -> anyhow::Result<PrepareOperationResponse>,
    render: impl FnOnce(&ServerClient) -> anyhow::Result<RenderOperationResponse>,
) -> anyhow::Result<DiffOutcome>
where
    PrepareOperationResponse: Into<crate::diff_viewer_client::PreparedRecipeBatch>,
    RenderOperationResponse: Into<crate::diff_viewer_client::RenderedDiffResult>,
{
    let client = ServerClient::connect()?;
    if artifact_only(raw) {
        return crate::diff_viewer_client::finish_render(render(&client)?, None);
    }

    match crate::diff_viewer_client::forward_prepared(prepare(&client)?) {
        Ok(outcome) => Ok(outcome),
        Err(error) => crate::diff_viewer_client::finish_render(render(&client)?, Some(&error)),
    }
}

fn artifact_only(raw: bool) -> bool {
    raw || !crate::viewer::has_display() || crate::viewer::no_open_requested()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_builds_a_triple_slash_url_for_a_unix_absolute_path() {
        assert_eq!(
            file_url(Path::new(
                "/home/user/.local/share/git-tools/diffs/r/h.html"
            )),
            "file:///home/user/.local/share/git-tools/diffs/r/h.html"
        );
    }

    #[test]
    fn file_url_normalizes_a_windows_style_path() {
        assert_eq!(
            file_url(Path::new(
                r"C:\Users\me\AppData\Local\git-tools\diffs\r\h.html"
            )),
            "file:///C:/Users/me/AppData/Local/git-tools/diffs/r/h.html"
        );
    }
}
