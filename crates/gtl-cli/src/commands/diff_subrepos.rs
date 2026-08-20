use std::{num::NonZeroU32, path::Path};

use gtl_wire::v1;

use crate::commands::diff::DiffOutcome;

pub fn run_scan(
    root: impl AsRef<Path>,
    last: Option<NonZeroU32>,
    include_worktrees: bool,
    raw: bool,
) -> anyhow::Result<DiffOutcome> {
    let root = std::path::absolute(root.as_ref())?
        .to_string_lossy()
        .into_owned();
    let target = last.map_or_else(unpushed_target, |count| v1::DiffTarget {
        selection: Some(v1::diff_target::Selection::LastCommitCount(count.get())),
    });
    let prepare = v1::PrepareSubrepositoriesDiffRequest {
        root: root.clone(),
        target: Some(target.clone()),
        include_linked_worktrees: include_worktrees,
    };
    let render = v1::RenderSubrepositoriesDiffRequest {
        root,
        target: Some(target),
        include_linked_worktrees: include_worktrees,
    };
    super::present(
        raw,
        |client| client.prepare_subrepositories_diff(prepare),
        |client| client.render_subrepositories_diff(render),
    )
}

pub fn run_managed_all(root: impl AsRef<Path>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let root = std::path::absolute(root.as_ref())?
        .to_string_lossy()
        .into_owned();
    super::present(
        raw,
        |client| client.prepare_projects_diff(v1::PrepareProjectsDiffRequest {}),
        |client| client.render_projects_diff(v1::RenderProjectsDiffRequest { root }),
    )
}

fn unpushed_target() -> v1::DiffTarget {
    v1::DiffTarget {
        selection: Some(v1::diff_target::Selection::Unpushed(v1::Empty {})),
    }
}
