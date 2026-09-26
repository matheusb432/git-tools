use std::path::{Path, PathBuf};

use anyhow::Context as _;

use crate::{commands::diff::DiffOutcome, server_client::ServerClient};

pub mod diff;
pub mod diff_subrepos;
pub mod managed;
pub mod merge_diff;
pub mod project_add;
pub mod project_status;
pub mod pull;
pub mod push_subrepos;
pub mod server_ctl;
pub mod sync;
pub mod tag;

pub(crate) fn repository_path(
    id: Option<&gtl_models::projects::catalogue::ProjectId>,
) -> anyhow::Result<PathBuf> {
    let Some(id) = id else {
        return canonical_working_directory();
    };
    let response = ServerClient::connect()?.get_project_repository(
        gtl_wire::v1::GetProjectRepositoryRequest {
            project_id: id.to_string(),
        },
    )?;
    let root = gtl_models::paths::RepositoryRoot::try_new(response.repository_root.into())
        .context("server returned an invalid project repository path")?;
    Ok(root.as_ref().to_path_buf())
}

pub(crate) fn canonical_working_directory() -> anyhow::Result<PathBuf> {
    let current = std::env::current_dir().context("resolving the current directory")?;
    std::fs::canonicalize(&current)
        .with_context(|| format!("canonicalizing current directory {}", current.display()))
}

/// Does not percent-encode trusted artifact paths.
pub(crate) fn file_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix('/') {
        format!("file:///{rest}")
    } else {
        format!("file:///{normalized}")
    }
}

pub(crate) fn present<PresentOperationResponse, RenderOperationResponse>(
    raw: bool,
    present: impl FnOnce(&ServerClient) -> anyhow::Result<PresentOperationResponse>,
    render: impl FnOnce(&ServerClient) -> anyhow::Result<RenderOperationResponse>,
) -> anyhow::Result<DiffOutcome>
where
    PresentOperationResponse: Into<crate::diff_viewer_client::PresentedDiffResult>,
    RenderOperationResponse: Into<crate::diff_viewer_client::RenderedDiffResult>,
{
    let client = ServerClient::connect()?;
    if artifact_only(raw) {
        return crate::diff_viewer_client::finish_render(render(&client)?);
    }
    crate::diff_viewer_client::finish_presentation(present(&client)?)
}

fn artifact_only(raw: bool) -> bool {
    raw || crate::viewer::no_open_requested()
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
