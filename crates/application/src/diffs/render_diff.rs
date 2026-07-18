//! The `render_diff` vertical slice: resolve a [`DiffTarget`] into a rendered,
//! stored artifact (or an "empty, skipped" outcome), carrying every user-facing
//! message out as [`Note`]s. The CLI and daemon call [`execute`] directly.

use std::path::{Path, PathBuf};

use domain::diffs::{AppliedExclusions, DiffExclusions, DiffKind};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget, DiffTargetRequest, DiffTargetRequestError, PinnedRange, View,
        range::DiffRanges,
        range_view::{RangePresentation, RangeView},
        sort_files_tree_order,
        util::{DiffData, assemble, exclusion_note, repo_name},
    },
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer, UserSettingsStore},
    shared::notes::Note,
};

/// Render a diff preview for `target` under `store_root`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiff {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
    pub target: DiffTargetRequest,
    #[serde(default)]
    pub name: Option<String>,
}

/// The outcome plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffResponse {
    pub outcome: RenderDiffOutcome,
    pub notes: Vec<Note>,
}

/// What a single render produced: a stored artifact, or a deliberately-skipped empty range.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderDiffOutcome {
    /// An artifact landed at `artifact`; `reused` when an identical one already existed.
    Rendered { artifact: PathBuf, reused: bool },
    /// The range was empty (no commits or changes); nothing was rendered.
    Empty,
}

/// Everything that can go wrong rendering a diff.
#[derive(Debug, thiserror::Error)]
pub enum RenderDiffError {
    #[error(transparent)]
    InvalidTarget(#[from] DiffTargetRequestError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

fn range_fast_path(
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    top: &str,
    store_root: &Path,
    target: &DiffTarget,
    theme: Option<&str>,
    excluded_extensions: &[String],
) -> anyhow::Result<Option<PathBuf>> {
    let Some((kind, base_sha, head_sha)) = resolved_range(source, top, target) else {
        return Ok(None); // worktree mode or unresolved ⇒ no fast-path
    };
    store.lookup_by_range(
        store_root,
        Path::new(top),
        kind,
        &base_sha,
        &head_sha,
        theme,
        excluded_extensions,
    )
}

// ! Returns (kind, base_sha, head_sha) only for pure commit ranges; None for
// ! worktree (Hash) mode. Mirrors build_view's range selection but skips assemble.
fn resolved_range(
    source: &impl DiffSource,
    top: &str,
    target: &DiffTarget,
) -> Option<(DiffKind, String, String)> {
    let repo = Path::new(top);
    let diff_range = match target {
        DiffTarget::Range {
            range,
            pinned: None,
        } => DiffRanges::exact(range).diff,
        DiffTarget::Last {
            count,
            pinned: None,
        } => DiffRanges::exact(format!("HEAD~{count}..HEAD")).diff,
        DiffTarget::Merge { base, pinned: None } => DiffRanges::merge(base).diff,
        DiffTarget::Unpushed { pinned: None } => {
            // No upstream ⇒ build_view falls back to Hash (worktree) mode; not fast-path
            // eligible, and the fallback warning is emitted there (once), not here.
            let upstream = source.upstream(repo).ok()?;
            DiffRanges::unpushed(&upstream).diff
        }
        // Pinned targets already carry resolved SHAs, but the fast path keys on
        // symbolic upstream resolution (`source.upstream`/`resolve_sha` against a
        // ref); a pinned target skips that resolution entirely, so it's simplest
        // (and correct) to treat every pinned target as fast-path ineligible, same
        // as `Base` (Hash mode ⇒ worktree, never fast-path eligible).
        DiffTarget::Range {
            pinned: Some(_), ..
        }
        | DiffTarget::Merge {
            pinned: Some(_), ..
        }
        | DiffTarget::Unpushed { pinned: Some(_) }
        | DiffTarget::Last {
            pinned: Some(_), ..
        }
        | DiffTarget::Base(_) => return None,
    };
    let kind = DiffKind::from_diff_range(&diff_range);
    if kind == DiffKind::WorkTree {
        return None;
    }
    let base = source.resolve_sha(repo, range_base(&diff_range)).ok()?;
    let head = source
        .resolve_sha(repo, diff_range.rsplit("..").next()?)
        .ok()?;
    Some((kind, base, head))
}

// ! head = text after the last `..`; worktree mode has no commit head ⇒ sentinel.
fn head_sha_for(source: &impl DiffSource, top: &str, range: &str) -> String {
    if range.contains("..") {
        let tip = range.rsplit("..").next().unwrap_or("HEAD");
        source.resolve_sha(Path::new(top), tip).unwrap_or_default()
    } else {
        "WORKTREE".to_string()
    }
}

/// Renders a diff through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderDiff,
    app_settings: &impl UserSettingsStore,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderDiffResponse, RenderDiffError> {
    let RenderDiff {
        cwd,
        store_root,
        target,
        name,
    } = req;
    let target = DiffTarget::try_from(target)?;
    let mut notes = Vec::new();
    let settings = app_settings.load();
    let theme = settings.theme().map(str::to_owned);
    let top = source.top_level(&cwd)?;
    let excluded = settings
        .diff_exclusions()
        .for_project_or_default(&repo_name(&top));

    // ! Fast-path: pure commit ranges are fully determined by resolved shas plus
    // ! the active rendering settings, so a prior identical artifact can be
    // ! reused without the expensive assemble — never across a config change.
    if name.is_none()
        && let Some(hit) = range_fast_path(
            source,
            store,
            &top,
            &store_root,
            &target,
            theme.as_deref(),
            excluded.extensions(),
        )?
    {
        notes.push(Note::info(format!(
            "diff-preview: reusing {}",
            hit.display()
        )));
        return Ok(RenderDiffResponse {
            outcome: RenderDiffOutcome::Rendered {
                artifact: hit,
                reused: true,
            },
            notes,
        });
    }

    let (mut view, summary) = build_view(
        source,
        &top,
        &target,
        theme.clone(),
        settings.diff_exclusions(),
        &mut notes,
    )?;
    if let Some(name) = &name {
        view.title.clone_from(name);
    }
    if !view.has_diff_content() {
        notes.push(Note::warn(format!(
            "diff-preview: {summary} — nothing to show (no commits or changes); skipping"
        )));
        return Ok(RenderDiffResponse {
            outcome: RenderDiffOutcome::Empty,
            notes,
        });
    }
    let file_count = view.files.len();
    let html = renderer.build_html(&view);

    let repo = Path::new(&top);
    let meta = ArtifactMeta {
        repo_root: PathBuf::from(&top),
        repo_name: view.repo_name.clone(),
        kind: DiffKind::from_diff_range(&view.cmd.range),
        base_sha: source
            .resolve_sha(repo, range_base(&view.cmd.range))
            .unwrap_or_default(),
        head_sha: head_sha_for(source, &top, &view.cmd.range),
        range_label: view.cmd.range.clone(),
        head_committed_at: source.committed_at(repo, "HEAD"),
        generated_at: clock.now_iso(),
        title: view.title.clone(),
        theme,
        excluded_extensions: excluded.extensions().to_vec(),
    };
    let placed = store.place(&store_root, &meta, &html)?;

    notes.push(Note::info(format!(
        "diff-preview: {summary}, {}",
        legacy_count_label(file_count, "file")
    )));
    notes.push(Note::info(format!("wrote {}", placed.path.display())));
    Ok(RenderDiffResponse {
        outcome: RenderDiffOutcome::Rendered {
            artifact: placed.path,
            reused: placed.reused,
        },
        notes,
    })
}

/// The resolved base label plus the io (real SHAs/refs) and view (display-safe)
/// ranges for a target, and whether it silently fell back to `main`.
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

/// Resolves `target` to its ranges: a pinned target computes verbatim over its
/// resolved SHAs (skipping symbolic verification entirely); an unpinned target
/// verifies/resolves symbolically as before, emitting the "no upstream"
/// fallback warning into `notes` when it applies.
fn resolve_target_ranges(
    source: &impl DiffSource,
    top: &str,
    target: &DiffTarget,
    notes: &mut Vec<Note>,
) -> anyhow::Result<ResolvedTarget> {
    let repo = Path::new(top);
    let resolved = match target {
        // Range, Last, and Unpushed pins are all exact ranges. Only their summary/upstream label
        // differs; range computation and presentation stay identical.
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
            source.verify_commit(repo, base)?;
            let short = source.short_ref(repo, base)?;
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
            source.verify_commit(repo, base)?;
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
            // * "last N" is just the HEAD~N..HEAD range; reuse the verified ExactRange plumbing.
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

/// Shared with diff-subrepos: builds the [`View`] + human summary for a target.
/// Emits the "no upstream" fallback warning and the exclusion note into `notes`.
pub fn build_view(
    source: &impl DiffSource,
    top: &str,
    target: &DiffTarget,
    theme: Option<String>,
    exclusions: &DiffExclusions,
    notes: &mut Vec<Note>,
) -> anyhow::Result<(View, String)> {
    let repo = Path::new(top);
    let branch = source.current_branch(repo)?;
    let repo_name = repo_name(top);
    let excluded = exclusions.for_project_or_default(&repo_name);

    let ResolvedTarget {
        base_ref,
        io_ranges,
        view_ranges,
        presentation,
        fallback_to_main,
    } = resolve_target_ranges(source, top, target, notes)?;
    let range_view = RangeView::new(&view_ranges.diff, presentation);

    let DiffData {
        commits,
        mut files,
        hidden_paths,
    } = assemble(source, repo, &io_ranges.diff, &io_ranges.log, excluded)?;
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
        theme,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    notes.extend(exclusion_note("diff-preview", &view));

    let summary = match target {
        DiffTarget::Range { .. } => base_ref.clone(),
        DiffTarget::Base(_) => format!("{base_ref}..working"),
        DiffTarget::Merge { .. } => format!("to merge into {base_ref}"),
        DiffTarget::Unpushed { .. } if fallback_to_main => format!("{base_ref}..working"),
        DiffTarget::Unpushed { .. } => legacy_unpushed_commit_label(view.commits.len()),
        DiffTarget::Last { count, .. } => {
            format!(
                "last {}",
                legacy_count_label(count.get() as usize, "commit")
            )
        }
    };
    Ok((view, summary))
}

struct DiffBase {
    ref_name: String,
    is_upstream: bool,
}

fn unpushed_or_main_base(
    source: &impl DiffSource,
    top: &str,
    notes: &mut Vec<Note>,
) -> anyhow::Result<DiffBase> {
    let repo = Path::new(top);
    match source.upstream(repo) {
        Ok(upstream) => Ok(DiffBase {
            ref_name: upstream,
            is_upstream: true,
        }),
        Err(upstream_error) => {
            source
                .verify_commit(repo, "main")
                .map_err(|_| upstream_error)?;
            notes.push(Note::warn(
                "diff-preview: no upstream; falling back to main",
            ));
            Ok(DiffBase {
                ref_name: "main".to_string(),
                is_upstream: false,
            })
        }
    }
}

fn verify_exact_range(source: &impl DiffSource, top: &str, range: &str) -> anyhow::Result<()> {
    let Some((start, end)) = range.split_once("..") else {
        anyhow::bail!("range must use <start>..<end>");
    };
    if start.trim().is_empty() || end.trim().is_empty() {
        anyhow::bail!("range must use <start>..<end>");
    }
    let repo = Path::new(top);
    source.verify_commit(repo, start)?;
    source.verify_commit(repo, end)?;
    Ok(())
}

// ! base = text before `..`/`...`; for worktree mode (no `..`) the whole string is the base.
fn range_base(range: &str) -> &str {
    range
        .split("..")
        .next()
        .unwrap_or(range)
        .trim_end_matches('.')
}

fn legacy_count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}(s)")
}

fn legacy_unpushed_commit_label(count: usize) -> String {
    format!("{count} unpushed commit(s)")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::{DiffExclusions, DiffKind};

    use super::{RenderDiff, RenderDiffError, RenderDiffOutcome, execute};
    use crate::{
        diffs::{DiffTarget, DiffTargetRequest},
        ports::AppSettings,
        shared::notes::Note,
        testing::{
            FakeDiffSource, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore,
            StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(source_top: &str, target: &DiffTarget) -> RenderDiff {
        RenderDiff {
            cwd: PathBuf::from(source_top),
            store_root: PathBuf::from("/store"),
            target: DiffTargetRequest::from(target),
            name: None,
        }
    }

    #[test]
    fn renders_and_stores_an_artifact_with_the_summary_notes() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            committed_at: "2026-07-02".into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo", &DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.outcome,
            RenderDiffOutcome::Rendered {
                artifact: PathBuf::from("/store/diffs/fake/artifact.html"),
                reused: false,
            }
        );
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "diff");
        assert_eq!(artifact.meta.repo_name, "repo");
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-preview: 1 unpushed commit(s), 1 file(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
    }

    #[test]
    fn render_uses_the_resolved_projects_settings_snapshot() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let app_settings = FixedUserSettingsStore::new(AppSettings::new(
            Some("night".into()),
            true,
            DiffExclusions::new(
                [
                    ("repo".to_string(), vec!["md".to_string()]),
                    ("defaults".to_string(), vec!["txt".to_string()]),
                ],
                None,
            ),
        ));

        execute(
            RenderDiff {
                cwd: PathBuf::from("/repo"),
                store_root: PathBuf::from("/store"),
                target: DiffTargetRequest::Unpushed,
                name: None,
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.excluded_extensions, vec!["md"]);
        assert!(artifact.html.contains("night"));
    }

    #[test]
    fn empty_view_returns_empty_with_the_skip_warning() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo", &DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(response.outcome, RenderDiffOutcome::Empty);
        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff-preview: 0 unpushed commit(s) — nothing to show (no commits or changes); skipping"
            )]
        );
    }

    #[test]
    fn pure_range_reuses_an_existing_artifact() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            shas: [
                ("a".to_string(), "sha-a".to_string()),
                ("b".to_string(), "sha-b".to_string()),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(
            DiffKind::TwoDot,
            "sha-a",
            "sha-b",
            None,
            &[],
            "/store/existing.html",
        );

        let response = execute(
            req(
                "/repo",
                &DiffTarget::Range {
                    range: "a..b".into(),
                    pinned: None,
                },
            ),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.outcome,
            RenderDiffOutcome::Rendered {
                artifact: PathBuf::from("/store/existing.html"),
                reused: true,
            }
        );
        assert_eq!(
            response.notes,
            vec![Note::info("diff-preview: reusing /store/existing.html")]
        );
    }

    #[test]
    fn fast_path_never_reuses_an_artifact_rendered_under_a_different_exclusion_set() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            shas: [
                ("a".to_string(), "sha-a".to_string()),
                ("b".to_string(), "sha-b".to_string()),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        // A hit exists for this range rendered WITHOUT exclusions…
        store.range_hit_insert(
            DiffKind::TwoDot,
            "sha-a",
            "sha-b",
            None,
            &[],
            "/store/unfiltered.html",
        );

        // …but this render runs with an md filter for the repo, so it must
        // recompute instead of serving the stale unfiltered artifact.
        let request = req(
            "/repo",
            &DiffTarget::Range {
                range: "a..b".into(),
                pinned: None,
            },
        );
        let app_settings = FixedUserSettingsStore::new(AppSettings::new(
            None,
            true,
            DiffExclusions::new([("repo".to_string(), vec!["md".to_string()])], None),
        ));
        let response = execute(
            request,
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered { reused: false, .. }
        ));
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(
            artifact.meta.excluded_extensions,
            vec!["md".to_string()],
            "the active set must be recorded for future range lookups"
        );
    }

    #[test]
    fn fast_path_never_reuses_an_artifact_rendered_under_a_different_theme() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            shas: [
                ("a".to_string(), "sha-a".to_string()),
                ("b".to_string(), "sha-b".to_string()),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(
            DiffKind::TwoDot,
            "sha-a",
            "sha-b",
            Some("dark"),
            &[],
            "/store/dark.html",
        );

        let response = execute(
            req(
                "/repo",
                &DiffTarget::Range {
                    range: "a..b".into(),
                    pinned: None,
                },
            ),
            &FixedUserSettingsStore::new(AppSettings::new(
                Some("light".to_string()),
                true,
                DiffExclusions::default(),
            )),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered { reused: false, .. }
        ));
    }

    #[test]
    fn fast_path_reuses_an_artifact_rendered_under_the_same_exclusion_set() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            shas: [
                ("a".to_string(), "sha-a".to_string()),
                ("b".to_string(), "sha-b".to_string()),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(
            DiffKind::TwoDot,
            "sha-a",
            "sha-b",
            None,
            &["md"],
            "/store/filtered.html",
        );

        let request = req(
            "/repo",
            &DiffTarget::Range {
                range: "a..b".into(),
                pinned: None,
            },
        );
        let app_settings = FixedUserSettingsStore::new(AppSettings::new(
            None,
            true,
            DiffExclusions::new([("repo".to_string(), vec!["md".to_string()])], None),
        ));
        let response = execute(
            request,
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.outcome,
            RenderDiffOutcome::Rendered {
                artifact: PathBuf::from("/store/filtered.html"),
                reused: true,
            }
        );
    }

    #[test]
    fn named_run_skips_the_fast_path_and_overrides_the_title() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            shas: [
                ("a".to_string(), "sha-a".to_string()),
                ("b".to_string(), "sha-b".to_string()),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        // A range hit exists, but a named run must ignore the fast-path entirely.
        store.range_hit_insert(
            DiffKind::TwoDot,
            "sha-a",
            "sha-b",
            None,
            &[],
            "/store/existing.html",
        );

        let mut request = req(
            "/repo",
            &DiffTarget::Range {
                range: "a..b".into(),
                pinned: None,
            },
        );
        request.name = Some("custom".into());
        let response = execute(
            request,
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered { reused: false, .. }
        ));
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "custom");
    }

    #[test]
    fn unpushed_without_upstream_warns_and_falls_back_to_main() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            known_revs: vec!["main".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo", &DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered { .. }
        ));
        assert_eq!(
            response.notes,
            vec![
                Note::warn("diff-preview: no upstream; falling back to main"),
                Note::info("diff-preview: main..working, 1 file(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
    }

    #[test]
    fn unknown_base_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let error = execute(
            req("/repo", &DiffTarget::Base("nope".into())),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect_err("unknown base errors");

        let RenderDiffError::Unexpected(err) = error else {
            panic!("an unknown revision is an execution error")
        };
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
