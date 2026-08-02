use std::path::Path;

use gtl_models::diffs::AppliedExclusions;

use super::DEFAULT_BASE;
use crate::{
    diffs::{
        PinnedRange, View,
        range::DiffRanges,
        range_view::{RangePresentation, RangeView},
        util::{DiffData, assemble, repo_name},
    },
    ports::GitClient,
};

pub(crate) struct MergeViewBuild {
    pub view: View,
    pub top: String,
    pub base: String,
    pub diff_range: String,
}

pub(super) fn build(
    source: &impl GitClient,
    cwd: &Path,
    base: Option<&str>,
    pinned: Option<&PinnedRange>,
    exclusions: &gtl_models::diffs::DiffExclusions,
) -> anyhow::Result<MergeViewBuild> {
    let top = source.top_level(cwd)?;
    let branch = source.current_branch(Path::new(&top))?;
    let repo_name = repo_name(&top);
    let excluded = exclusions.for_project_or_default(&repo_name);
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE);

    #[expect(
        clippy::single_match_else,
        reason = "the repository rule requires match for conditional initializers"
    )]
    let (io_ranges, view_ranges) = match pinned {
        Some(pin) => (
            DiffRanges::exact(pin.git_range()),
            DiffRanges::exact(pin.display_range()),
        ),
        None => {
            source.verify_commit(Path::new(&top), base)?;
            let symbolic = DiffRanges::merge(base);
            (symbolic.clone(), symbolic)
        }
    };
    let range_view = RangeView::new(&view_ranges.diff, RangePresentation::Merge);
    let DiffData {
        commits,
        files,
        hidden_paths,
    } = assemble(
        source,
        Path::new(&top),
        &io_ranges.diff,
        &io_ranges.log,
        excluded,
    )?;

    let view = View {
        repo_name,
        repo_root: top.clone(),
        branch,
        upstream: base.to_string(),
        title: range_view.title,
        cmd: range_view.cmd,
        commits_label: range_view.commits_label,
        foot: range_view.foot,
        commits,
        files,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    Ok(MergeViewBuild {
        view,
        top,
        base: base.to_string(),
        diff_range: io_ranges.diff,
    })
}
