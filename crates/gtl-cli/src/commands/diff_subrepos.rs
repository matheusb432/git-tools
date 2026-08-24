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
    let present = v1::PresentSubrepositoryDiffsRequest {
        root: root.clone(),
        target: Some(target.clone()),
        include_linked_worktrees: include_worktrees,
    };
    let render = v1::RenderSubrepositoryDiffsRequest {
        root,
        target: Some(target),
        include_linked_worktrees: include_worktrees,
    };
    super::present(
        raw,
        |client| client.present_subrepository_diffs(present),
        |client| client.render_subrepository_diffs(render),
    )
}

pub fn run_managed_all(root: impl AsRef<Path>, raw: bool) -> anyhow::Result<DiffOutcome> {
    let root = std::path::absolute(root.as_ref())?
        .to_string_lossy()
        .into_owned();
    let presentation_root = root.clone();
    super::present(
        raw,
        |client| {
            client.present_project_repository_diffs(v1::PresentProjectRepositoryDiffsRequest {
                root: presentation_root,
            })
        },
        |client| {
            client.render_project_repository_diffs(v1::RenderProjectRepositoryDiffsRequest { root })
        },
    )
}

fn unpushed_target() -> v1::DiffTarget {
    v1::DiffTarget {
        selection: Some(v1::diff_target::Selection::Unpushed(v1::Empty {})),
    }
}
