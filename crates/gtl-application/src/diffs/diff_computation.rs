use std::path::Path;

use gtl_models::diffs::{AppliedExclusions, DiffExclusions};

use crate::{
    diffs::{
        DiffTarget, PinnedRange, View,
        logic::{
            assemble::{DiffData, assemble},
            exclusions,
            range::DiffRanges,
            range_view::{RangePresentation, RangeView},
            view::sort_files_tree_order,
        },
    },
    ports::GitClient,
    shared::{notes::Note, repository_name::from_path},
};

pub(super) struct DiffComputation {
    pub(super) view: View,
    pub(super) summary: String,
    pub(super) notes: Vec<Note>,
}

struct ResolvedTarget {
    base_ref: String,
    io_ranges: DiffRanges,
    view_ranges: DiffRanges,
    presentation: RangePresentation,
    fallback_to_main: bool,
}

impl ResolvedTarget {
    fn pinned(pin: &PinnedRange, base_ref: String, presentation: RangePresentation) -> Self {
        Self {
            base_ref,
            io_ranges: DiffRanges::exact(pin.git_range()),
            view_ranges: DiffRanges::exact(pin.display_range()),
            presentation,
            fallback_to_main: false,
        }
    }

    fn same_ranges(
        base_ref: String,
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
    source: &impl GitClient,
    top: &str,
    target: &DiffTarget,
    exclusions: &DiffExclusions,
) -> anyhow::Result<DiffComputation> {
    let mut notes = Vec::new();
    let repo_path = Path::new(top);
    let branch = source.current_branch(repo_path)?;
    let repo_name = from_path(top);
    let excluded = exclusions.for_project_or_default(&repo_name);

    let ResolvedTarget {
        base_ref,
        io_ranges,
        view_ranges,
        presentation,
        fallback_to_main,
    } = resolve_target_ranges(source, top, target, &mut notes)?;
    let range_view = RangeView::new(&view_ranges.diff, presentation);

    let DiffData {
        commits,
        mut files,
        hidden_paths,
    } = assemble(source, repo_path, &io_ranges.diff, &io_ranges.log, excluded)?;
    sort_files_tree_order(&mut files);

    let view = View {
        repo_name: repo_name.clone(),
        repo_root: top.to_string(),
        branch,
        upstream: base_ref.clone(),
        title: range_view.title,
        cmd: range_view.cmd,
        commits_label: range_view.commits_label,
        foot: range_view.foot,
        commits,
        files,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    notes.extend(exclusions::note("diff-preview", &view));

    let summary = match target {
        DiffTarget::Range { .. } => base_ref.clone(),
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
    source: &impl GitClient,
    top: &str,
    target: &DiffTarget,
    notes: &mut Vec<Note>,
) -> anyhow::Result<ResolvedTarget> {
    let repo_path = Path::new(top);
    let resolved = match target {
        DiffTarget::Range {
            pinned: Some(pin), ..
        }
        | DiffTarget::Last {
            pinned: Some(pin), ..
        } => ResolvedTarget::pinned(pin, pin.display_range(), RangePresentation::Exact),
        DiffTarget::Range {
            range,
            pinned: None,
        } => {
            verify_exact_range(source, top, range)?;
            ResolvedTarget::same_ranges(
                range.clone(),
                DiffRanges::exact(range),
                RangePresentation::Exact,
                false,
            )
        }
        DiffTarget::Base(base) => {
            source.verify_commit(repo_path, base)?;
            let short = source.short_ref(repo_path, base)?;
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
            source.verify_commit(repo_path, base)?;
            ResolvedTarget::same_ranges(
                base.clone(),
                DiffRanges::merge(base),
                RangePresentation::Merge,
                false,
            )
        }
        DiffTarget::Unpushed { pinned: Some(pin) } => {
            ResolvedTarget::pinned(pin, pin.display_base(), RangePresentation::Exact)
        }
        DiffTarget::Unpushed { pinned: None } => {
            let base = unpushed_or_main_base(source, top, notes)?;
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
            let range = format!("HEAD~{count}..HEAD");
            verify_exact_range(source, top, &range)?;
            ResolvedTarget::same_ranges(
                range.clone(),
                DiffRanges::exact(range),
                RangePresentation::Exact,
                false,
            )
        }
    };
    Ok(resolved)
}

struct DiffBase {
    ref_name: String,
    is_upstream: bool,
}

fn unpushed_or_main_base(
    source: &impl GitClient,
    top: &str,
    notes: &mut Vec<Note>,
) -> anyhow::Result<DiffBase> {
    let repo_path = Path::new(top);
    match source.upstream(repo_path) {
        Ok(crate::ports::GitEffect::Applied(upstream)) => Ok(DiffBase {
            ref_name: upstream,
            is_upstream: true,
        }),
        Ok(crate::ports::GitEffect::Rejected(upstream_error)) => {
            source
                .verify_commit(repo_path, "main")
                .map_err(|_| anyhow::anyhow!(upstream_error))?;
            notes.push(Note::warn(
                "diff-preview: no upstream; falling back to main",
            ));
            Ok(DiffBase {
                ref_name: "main".to_string(),
                is_upstream: false,
            })
        }
        Err(error) => Err(error),
    }
}

fn verify_exact_range(source: &impl GitClient, top: &str, range: &str) -> anyhow::Result<()> {
    let Some((start, end)) = range.split_once("..") else {
        anyhow::bail!("range must use <start>..<end>");
    };
    if start.trim().is_empty() || end.trim().is_empty() {
        anyhow::bail!("range must use <start>..<end>");
    }
    let repo_path = Path::new(top);
    source.verify_commit(repo_path, start)?;
    source.verify_commit(repo_path, end)?;
    Ok(())
}
