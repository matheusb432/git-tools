use gtl_models::{
    diffs::AppliedExclusions,
    git::{GitDiffSpec, GitRevision},
    paths::RepositoryRoot,
};

use crate::{
    diffs::{
        PinnedRange, View,
        assemble::{DiffData, assemble},
        range::DiffRanges,
        range_view::{RangePresentation, RangeView},
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
    exclusions: &gtl_models::diffs::DiffExclusions,
) -> anyhow::Result<MergeViewBuild> {
    let branch = git.current_branch(top)?;
    let repo_name = top.project_name();
    let excluded = exclusions.for_project_or_default(&repo_name);
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
    let range_view = RangeView::new(&view_ranges.diff, RangePresentation::Merge);
    let DiffData {
        commits,
        files,
        hidden_paths,
        full_context,
    } = assemble(git, top, &io_ranges.diff, &io_ranges.log, excluded)?;

    let view = View {
        repo_name,
        repo_root: top.clone(),
        branch,
        upstream: base.clone(),
        title: range_view.title,
        cmd: range_view.cmd,
        commits_label: range_view.commits_label,
        foot: range_view.foot,
        commits,
        files,
        full_context,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    Ok(MergeViewBuild {
        view,
        top: top.clone(),
        base,
        diff_range: io_ranges.diff,
    })
}
