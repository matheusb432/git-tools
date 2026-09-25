use gtl_models::{
    diffs::{AppliedExtensionFilter, CommitIdAbbreviation, DiffViewTitle, ExtensionFilter},
    git::{GitRange, GitRevision},
    paths::RepositoryRoot,
};

use crate::{
    diffs::{
        DiffTarget, PinnedRange, View,
        assemble::{DiffData, assemble},
        extension_filter_note,
        range::DiffRanges,
        range_view::RangeView,
        view::sort_files_tree_order,
    },
    ports::GitClient,
    shared::notes::Note,
};

pub(super) struct DiffComputation {
    pub(super) view: View,
    pub(super) summary: String,
    pub(super) notes: Vec<Note>,
}

struct ResolvedTarget {
    base_ref: GitRevision,
    io_ranges: DiffRanges,
    view_ranges: DiffRanges,
    title: DiffViewTitle,
    fallback_to_branch: bool,
}

impl ResolvedTarget {
    fn pinned(pin: &PinnedRange, base_ref: GitRevision, title: DiffViewTitle) -> Self {
        Self {
            base_ref,
            io_ranges: DiffRanges::exact(pin.to_git_range()),
            view_ranges: DiffRanges::exact(pin.to_display_range()),
            title,
            fallback_to_branch: false,
        }
    }

    fn same_ranges(
        base_ref: GitRevision,
        ranges: DiffRanges,
        title: DiffViewTitle,
        fallback_to_branch: bool,
    ) -> Self {
        Self {
            base_ref,
            io_ranges: ranges.clone(),
            view_ranges: ranges,
            title,
            fallback_to_branch,
        }
    }
}

pub(super) fn build(
    git: &impl GitClient,
    top: &RepositoryRoot,
    target: &DiffTarget,
    filter: &ExtensionFilter,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<DiffComputation, crate::projects::comparison::ComparisonError> {
    let mut notes = Vec::new();
    let branch = git.current_branch(top)?;
    let repo_name = top.project_name();

    let ResolvedTarget {
        base_ref,
        io_ranges,
        view_ranges,
        title,
        fallback_to_branch,
    } = resolve_target_ranges(git, top, target, &mut notes, comparisons)?;
    let range_view = RangeView::new(&view_ranges.diff, title);

    let DiffData {
        commits,
        mut files,
        hidden_paths,
        full_context,
    } = assemble(git, top, &io_ranges.diff, io_ranges.log.as_ref(), filter)?;
    sort_files_tree_order(&mut files);

    let view = View {
        file_filter: crate::diffs::file_filter::DiffFileFilter::new(
            io_ranges.diff.clone(),
            filter.clone(),
        ),
        repo_name: repo_name.clone(),
        repo_root: top.clone(),
        branch,
        upstream: base_ref.clone(),
        title: range_view.title,
        cmd: range_view.cmd,
        foot: range_view.foot,
        commits,
        files,
        full_context,
        extension_filter: AppliedExtensionFilter::from_hidden(filter, hidden_paths),
    };
    notes.extend(extension_filter_note::note("diff-artifact", &view));

    let summary = match target {
        DiffTarget::Range { .. } => base_ref.to_string(),
        DiffTarget::Base(_) => format!("{base_ref}..working"),
        DiffTarget::Merge { .. } => format!("to merge into {base_ref}"),
        DiffTarget::Unpushed { .. } if fallback_to_branch => {
            format!("branch changes against {base_ref}")
        }
        DiffTarget::Unpushed { .. } => format!("{} unpushed commit(s)", view.commits.len()),
        DiffTarget::Last { count, .. } => format!("last {count} commit(s)"),
    };
    Ok(DiffComputation {
        view,
        summary,
        notes,
    })
}

fn resolve_target_ranges(
    git: &impl GitClient,
    top: &RepositoryRoot,
    target: &DiffTarget,
    notes: &mut Vec<Note>,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<ResolvedTarget, crate::projects::comparison::ComparisonError> {
    let resolved = match target {
        DiffTarget::Range {
            pinned: Some(pin), ..
        }
        | DiffTarget::Last {
            pinned: Some(pin), ..
        } => ResolvedTarget::pinned(
            pin,
            GitRevision::from(&pin.to_display_range()),
            DiffViewTitle::Diff,
        ),
        DiffTarget::Range {
            range,
            pinned: None,
        } => {
            verify_exact_range(git, top, range)?;
            ResolvedTarget::same_ranges(
                GitRevision::from(range),
                DiffRanges::exact(range.clone()),
                DiffViewTitle::Diff,
                false,
            )
        }
        DiffTarget::Base(base)
            if *base == GitRevision::head() && !git.revision_exists(top, base)? =>
        {
            let mut ranges = DiffRanges::working_tree(base);
            ranges.log = None;
            ResolvedTarget::same_ranges(base.clone(), ranges, DiffViewTitle::Diff, false)
        }
        DiffTarget::Base(base) => {
            git.verify_commit(top, base)?;
            let short = git.resolve_commit_id(top, base)?;
            let short =
                GitRevision::abbreviated_commit(&short, CommitIdAbbreviation::TenCharacters);
            ResolvedTarget {
                base_ref: short.clone(),
                io_ranges: DiffRanges::working_tree(base),
                view_ranges: DiffRanges::working_tree(&short),
                title: DiffViewTitle::Diff,
                fallback_to_branch: false,
            }
        }
        DiffTarget::Merge {
            base,
            pinned: Some(pin),
        } => ResolvedTarget::pinned(pin, base.clone(), DiffViewTitle::MergeDiff),
        DiffTarget::Merge { base, pinned: None } => {
            git.verify_commit(top, base)?;
            ResolvedTarget::same_ranges(
                base.clone(),
                DiffRanges::merge(base),
                DiffViewTitle::MergeDiff,
                false,
            )
        }
        DiffTarget::Unpushed { pinned: Some(pin) } => {
            ResolvedTarget::pinned(pin, pin.to_display_base(), DiffViewTitle::Diff)
        }
        DiffTarget::Unpushed { pinned: None } => {
            let comparison = crate::projects::comparison::resolve(top, git, comparisons)?;
            let base = comparison.reference();
            let (ranges, fallback) = match comparison {
                crate::projects::comparison::ResolvedComparison::Upstream { .. } => {
                    (DiffRanges::unpushed(&base), false)
                }
                crate::projects::comparison::ResolvedComparison::Branch { branch, .. } => {
                    notes.push(Note::info(format!(
                        "diff-artifact: no upstream; comparing branch changes against {branch}"
                    )));
                    (DiffRanges::merge(&base), true)
                }
            };
            ResolvedTarget::same_ranges(base, ranges, DiffViewTitle::Diff, fallback)
        }

        DiffTarget::Last {
            count,
            pinned: None,
        } => {
            let range = GitRange::head_commits(*count);
            verify_exact_range(git, top, &range)?;
            ResolvedTarget::same_ranges(
                GitRevision::from(&range),
                DiffRanges::exact(range),
                DiffViewTitle::Diff,
                false,
            )
        }
    };
    Ok(resolved)
}

fn verify_exact_range(
    git: &impl GitClient,
    top: &RepositoryRoot,
    range: &GitRange,
) -> anyhow::Result<()> {
    let Some((start, end)) = range.as_ref().split_once("..") else {
        anyhow::bail!("range must use <start>..<end>");
    };
    if start.trim().is_empty() || end.trim().is_empty() {
        anyhow::bail!("range must use <start>..<end>");
    }
    let start = GitRevision::try_new(start.to_owned())?;
    let end = GitRevision::try_new(end.to_owned())?;
    git.verify_commit(top, &start)?;
    git.verify_commit(top, &end)?;
    Ok(())
}
