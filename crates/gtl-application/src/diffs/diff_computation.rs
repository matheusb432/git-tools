use gtl_models::{
    diffs::{AppliedExclusions, CommitIdAbbreviation, DiffExclusions},
    git::{GitRange, GitRevision},
    paths::RepositoryRoot,
};

use crate::{
    diffs::{
        DiffTarget, PinnedRange, View,
        assemble::{DiffData, assemble},
        exclusions,
        range::DiffRanges,
        range_view::{RangePresentation, RangeView},
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
    presentation: RangePresentation,
    fallback_to_main: bool,
}

impl ResolvedTarget {
    fn pinned(pin: &PinnedRange, base_ref: GitRevision, presentation: RangePresentation) -> Self {
        Self {
            base_ref,
            io_ranges: DiffRanges::exact(pin.to_git_range()),
            view_ranges: DiffRanges::exact(pin.to_display_range()),
            presentation,
            fallback_to_main: false,
        }
    }

    fn same_ranges(
        base_ref: GitRevision,
        ranges: DiffRanges,
        presentation: RangePresentation,
        fallback_to_main: bool,
    ) -> Self {
        Self {
            base_ref,
            io_ranges: ranges.clone(),
            view_ranges: ranges,
            presentation,
            fallback_to_main,
        }
    }
}

pub(super) fn build(
    git: &impl GitClient,
    top: &RepositoryRoot,
    target: &DiffTarget,
    exclusions: &DiffExclusions,
) -> anyhow::Result<DiffComputation> {
    let mut notes = Vec::new();
    let branch = git.current_branch(top)?;
    let repo_name = top.project_name();
    let excluded = exclusions.for_project_or_default(&repo_name);

    let ResolvedTarget {
        base_ref,
        io_ranges,
        view_ranges,
        presentation,
        fallback_to_main,
    } = resolve_target_ranges(git, top, target, &mut notes)?;
    let range_view = RangeView::new(&view_ranges.diff, presentation);

    let DiffData {
        commits,
        mut files,
        hidden_paths,
        full_context,
    } = assemble(git, top, &io_ranges.diff, &io_ranges.log, excluded)?;
    sort_files_tree_order(&mut files);

    let view = View {
        repo_name: repo_name.clone(),
        repo_root: top.clone(),
        branch,
        upstream: base_ref.clone(),
        title: range_view.title,
        cmd: range_view.cmd,
        commits_label: range_view.commits_label,
        foot: range_view.foot,
        commits,
        files,
        full_context,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    notes.extend(exclusions::note("diff-artifact", &view));

    let summary = match target {
        DiffTarget::Range { .. } => base_ref.to_string(),
        DiffTarget::Base(_) => format!("{base_ref}..working"),
        DiffTarget::Merge { .. } => format!("to merge into {base_ref}"),
        DiffTarget::Unpushed { .. } if fallback_to_main => format!("{base_ref}..working"),
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
) -> anyhow::Result<ResolvedTarget> {
    let resolved = match target {
        DiffTarget::Range {
            pinned: Some(pin), ..
        }
        | DiffTarget::Last {
            pinned: Some(pin), ..
        } => ResolvedTarget::pinned(
            pin,
            GitRevision::from(&pin.to_display_range()),
            RangePresentation::Exact,
        ),
        DiffTarget::Range {
            range,
            pinned: None,
        } => {
            verify_exact_range(git, top, range)?;
            ResolvedTarget::same_ranges(
                GitRevision::from(range),
                DiffRanges::exact(range.clone()),
                RangePresentation::Exact,
                false,
            )
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
                presentation: RangePresentation::WorkingTree,
                fallback_to_main: false,
            }
        }
        DiffTarget::Merge {
            base,
            pinned: Some(pin),
        } => ResolvedTarget::pinned(pin, base.clone(), RangePresentation::Merge),
        DiffTarget::Merge { base, pinned: None } => {
            git.verify_commit(top, base)?;
            ResolvedTarget::same_ranges(
                base.clone(),
                DiffRanges::merge(base),
                RangePresentation::Merge,
                false,
            )
        }
        DiffTarget::Unpushed { pinned: Some(pin) } => {
            ResolvedTarget::pinned(pin, pin.to_display_base(), RangePresentation::Exact)
        }
        DiffTarget::Unpushed { pinned: None } => {
            let base = unpushed_or_main_base(git, top, notes)?;
            let (ranges, presentation) = if base.is_upstream {
                (
                    DiffRanges::unpushed(&base.ref_name),
                    RangePresentation::Unpushed,
                )
            } else {
                (
                    DiffRanges::working_tree(&base.ref_name),
                    RangePresentation::WorkingTree,
                )
            };
            ResolvedTarget::same_ranges(base.ref_name, ranges, presentation, !base.is_upstream)
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
                RangePresentation::Exact,
                false,
            )
        }
    };
    Ok(resolved)
}

struct DiffBase {
    ref_name: GitRevision,
    is_upstream: bool,
}

fn unpushed_or_main_base(
    git: &impl GitClient,
    top: &RepositoryRoot,
    notes: &mut Vec<Note>,
) -> anyhow::Result<DiffBase> {
    match git.upstream(top) {
        Ok(crate::ports::GitEffect::Applied(upstream)) => Ok(DiffBase {
            ref_name: GitRevision::from(&upstream),
            is_upstream: true,
        }),
        Ok(crate::ports::GitEffect::Rejected(upstream_error)) => {
            git.verify_commit(top, &GitRevision::main())
                .map_err(|_| anyhow::anyhow!(upstream_error))?;
            notes.push(Note::warn(
                "diff-artifact: no upstream; falling back to main",
            ));
            Ok(DiffBase {
                ref_name: GitRevision::main(),
                is_upstream: false,
            })
        }
        Err(error) => Err(error),
    }
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
