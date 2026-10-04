use gtl_models::{
    diffs::{AppliedExtensionFilter, DiffViewTitle, ExtensionFilter},
    git::{GitDiffSpec, GitRevision},
    paths::RepositoryRoot,
    timestamps::MachineTimestamp,
};

use crate::{
    diffs::{
        PinnedRange, RepositoryOrigin, View, ViewOrigin,
        assemble::{DiffData, assemble},
        range::DiffRanges,
        range_view::RangeView,
    },
    ports::GitClient,
};

pub(super) struct MergeViewBuild {
    pub(super) view: View,
    pub(super) top: RepositoryRoot,
    pub(super) base: GitRevision,
    pub(super) diff_range: GitDiffSpec,
}

pub(super) fn build(
    git: &impl GitClient,
    top: &RepositoryRoot,
    base: Option<&GitRevision>,
    pinned: Option<&PinnedRange>,
    filter: &ExtensionFilter,
    changes_since: Option<&MachineTimestamp>,
) -> anyhow::Result<MergeViewBuild> {
    let branch = git.current_branch(top)?;
    let repo_name = top.project_name();
    let base = base.cloned().unwrap_or_else(GitRevision::main);

    let (io_ranges, view_ranges) = pinned.map_or_else(
        || -> anyhow::Result<_> {
            git.verify_commit(top, &base)?;
            let symbolic = DiffRanges::merge(&base);
            Ok((symbolic.clone(), symbolic))
        },
        |pin| {
            Ok((
                DiffRanges::exact(pin.to_git_range()),
                DiffRanges::exact(pin.to_display_range()),
            ))
        },
    )?;
    let range_view = RangeView::new(&view_ranges.diff, DiffViewTitle::MergeDiff);
    let DiffData {
        spec,
        commits,
        files,
        hidden_paths,
        full_context,
    } = assemble(
        git,
        top,
        &io_ranges.diff,
        io_ranges.log.as_ref(),
        filter,
        changes_since,
    )?;

    let view = View {
        file_filter: crate::diffs::file_filter::DiffFileFilter::new(spec.clone(), filter.clone()),
        origin: ViewOrigin::Repository(RepositoryOrigin {
            name: repo_name,
            root: top.clone(),
            branch,
            upstream: base.clone(),
        }),
        title: range_view.title,
        cmd: range_view.cmd,
        foot: range_view.foot,
        commits,
        files,
        full_context,
        extension_filter: AppliedExtensionFilter::from_hidden(filter, hidden_paths),
    };
    Ok(MergeViewBuild {
        view,
        top: top.clone(),
        base,
        diff_range: spec,
    })
}
