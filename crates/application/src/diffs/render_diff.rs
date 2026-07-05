//! The `render_diff` vertical slice: resolve a [`DiffTarget`] into a rendered,
//! stored artifact (or an "empty, skipped" outcome), carrying every user-facing
//! message out as [`Note`]s. The cli's `gtl diff` and `diff-subrepos` call this
//! in-process; a future daemon dispatches it through [`cqrsy::Handler`].

use std::path::{Path, PathBuf};

use cqrsy::Handler;
use domain::diffs::{DiffKind, DiffTarget, Mode, View, ranges, sort_files_tree_order};

use crate::{
    diffs::util::{DiffData, assemble},
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render a diff preview for `target` under `store_root`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq, cqrsy::Command)]
#[command(out = RenderDiffResponse, err = RenderDiffError)]
pub struct RenderDiff {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
    pub target: DiffTarget,
    pub name: Option<String>,
    pub theme: Option<String>,
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
    Unexpected(#[from] anyhow::Error),
}

/// Handles [`RenderDiff`] by driving the diff engine through its ports.
#[derive(Clone)]
pub struct RenderDiffHandler<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> {
    pub source: S,
    pub store: A,
    pub renderer: R,
    pub clock: C,
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> RenderDiffHandler<S, A, R, C> {
    fn range_fast_path(
        &self,
        top: &str,
        store_root: &Path,
        target: &DiffTarget,
    ) -> anyhow::Result<Option<PathBuf>> {
        let Some((kind, base_sha, head_sha)) = self.resolved_range(top, target) else {
            return Ok(None); // worktree mode or unresolved ⇒ no fast-path
        };
        self.store
            .lookup_by_range(store_root, Path::new(top), kind, &base_sha, &head_sha)
    }

    // ! Returns (kind, base_sha, head_sha) only for pure commit ranges; None for
    // ! worktree (Hash) mode. Mirrors build_view's range selection but skips assemble.
    fn resolved_range(&self, top: &str, target: &DiffTarget) -> Option<(DiffKind, String, String)> {
        let repo = Path::new(top);
        let diff_range = match target {
            DiffTarget::Range(r) => ranges(r, Mode::ExactRange).diff_range,
            DiffTarget::Last(n) => ranges(&format!("HEAD~{n}..HEAD"), Mode::ExactRange).diff_range,
            DiffTarget::Merge(b) => ranges(b, Mode::Merge).diff_range,
            DiffTarget::Unpushed => {
                // No upstream ⇒ build_view falls back to Hash (worktree) mode; not fast-path
                // eligible, and the fallback warning is emitted there (once), not here.
                let upstream = self.source.upstream(repo).ok()?;
                ranges(&upstream, Mode::Unpushed).diff_range
            }
            DiffTarget::Base(_) => return None, // Hash mode ⇒ worktree
        };
        let kind = DiffKind::from_diff_range(&diff_range);
        if kind == DiffKind::WorkTree {
            return None;
        }
        let base = self
            .source
            .resolve_sha(repo, range_base(&diff_range))
            .ok()?;
        let head = self
            .source
            .resolve_sha(repo, diff_range.rsplit("..").next()?)
            .ok()?;
        Some((kind, base, head))
    }

    // ! head = text after the last `..`; worktree mode has no commit head ⇒ sentinel.
    fn head_sha_for(&self, top: &str, range: &str) -> String {
        if range.contains("..") {
            let tip = range.rsplit("..").next().unwrap_or("HEAD");
            self.source
                .resolve_sha(Path::new(top), tip)
                .unwrap_or_default()
        } else {
            "WORKTREE".to_string()
        }
    }
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> Handler<RenderDiff>
    for RenderDiffHandler<S, A, R, C>
{
    async fn handle(&self, req: RenderDiff) -> Result<RenderDiffResponse, RenderDiffError> {
        let mut notes = Vec::new();
        let top = self.source.top_level(&req.cwd)?;

        // ! Fast-path: pure commit ranges are fully determined by resolved shas, so a
        // ! prior identical artifact can be reused without the expensive assemble.
        if req.name.is_none()
            && let Some(hit) = self.range_fast_path(&top, &req.store_root, &req.target)?
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
            &self.source,
            &top,
            &req.target,
            req.theme.clone(),
            &mut notes,
        )?;
        if let Some(name) = &req.name {
            view.title = name.clone();
        }
        if view.is_empty() {
            notes.push(Note::warn(format!(
                "diff-preview: {summary} — nothing to show (no commits or changes); skipping"
            )));
            return Ok(RenderDiffResponse {
                outcome: RenderDiffOutcome::Empty,
                notes,
            });
        }
        let file_count = view.files.len();
        let html = self.renderer.build_html(&view);

        let repo = Path::new(&top);
        let meta = ArtifactMeta {
            repo_root: PathBuf::from(&top),
            repo_name: view.repo_name.clone(),
            kind: DiffKind::from_diff_range(&view.cmd.range),
            base_sha: self
                .source
                .resolve_sha(repo, range_base(&view.cmd.range))
                .unwrap_or_default(),
            head_sha: self.head_sha_for(&top, &view.cmd.range),
            range_label: view.cmd.range.clone(),
            head_committed_at: self.source.committed_at(repo, "HEAD"),
            generated_at: self.clock.now_iso(),
            title: view.title.clone(),
        };
        let placed = self.store.place(&req.store_root, &meta, &html)?;

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
}

/// Shared with diff-subrepos: builds the [`View`] + human summary for a target.
/// Emits the "no upstream" fallback warning into `notes`.
pub fn build_view(
    source: &impl DiffSource,
    top: &str,
    target: &DiffTarget,
    theme: Option<String>,
    notes: &mut Vec<Note>,
) -> anyhow::Result<(View, String)> {
    let repo = Path::new(top);
    let branch = source.current_branch(repo)?;
    let repo_name = repo_name(top);

    let (base_ref, io_ranges, view_ranges, fallback_to_main) = match target {
        DiffTarget::Range(range) => {
            verify_exact_range(source, top, range)?;
            (
                range.clone(),
                ranges(range, Mode::ExactRange),
                ranges(range, Mode::ExactRange),
                false,
            )
        }
        DiffTarget::Base(base) => {
            source.verify_commit(repo, base)?;
            let short = source.short_ref(repo, base)?;
            (
                short.clone(),
                ranges(base, Mode::Hash),
                ranges(&short, Mode::Hash),
                false,
            )
        }
        DiffTarget::Merge(base) => {
            source.verify_commit(repo, base)?;
            (
                base.clone(),
                ranges(base, Mode::Merge),
                ranges(base, Mode::Merge),
                false,
            )
        }
        DiffTarget::Unpushed => {
            let base = unpushed_or_main_base(source, top, notes)?;
            let mode = if base.is_upstream {
                Mode::Unpushed
            } else {
                Mode::Hash
            };
            (
                base.ref_name.clone(),
                ranges(&base.ref_name, mode),
                ranges(&base.ref_name, mode),
                !base.is_upstream,
            )
        }
        DiffTarget::Last(count) => {
            // * "last N" is just the HEAD~N..HEAD range; reuse the verified ExactRange plumbing.
            let range = format!("HEAD~{count}..HEAD");
            verify_exact_range(source, top, &range)?;
            (
                range.clone(),
                ranges(&range, Mode::ExactRange),
                ranges(&range, Mode::ExactRange),
                false,
            )
        }
    };

    let DiffData { commits, mut files } = assemble(
        source,
        repo,
        &io_ranges.diff_args,
        &io_ranges.diff_range,
        &io_ranges.log_range,
    )?;
    sort_files_tree_order(&mut files);

    let view = View {
        repo_name: repo_name.clone(),
        repo_root: top.to_string(),
        branch,
        upstream: base_ref.clone(),
        title: view_ranges.title,
        cmd: view_ranges.cmd,
        commits_label: view_ranges.commits_label,
        foot: view_ranges.foot,
        commits,
        files,
        theme,
    };

    let summary = match target {
        DiffTarget::Range(_) => base_ref.clone(),
        DiffTarget::Base(_) => format!("{base_ref}..working"),
        DiffTarget::Merge(_) => format!("to merge into {base_ref}"),
        DiffTarget::Unpushed if fallback_to_main => format!("{base_ref}..working"),
        DiffTarget::Unpushed => legacy_unpushed_commit_label(view.commits.len()),
        DiffTarget::Last(count) => {
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

fn repo_name(top: &str) -> String {
    Path::new(top)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
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

    use cqrsy::send_now;
    use domain::diffs::{Commit, DiffKind, DiffTarget};

    use super::{RenderDiff, RenderDiffError, RenderDiffHandler, RenderDiffOutcome};
    use crate::{
        shared::notes::Note,
        testing::{FakeDiffSource, FixedClock, InMemoryArtifactStore, StubRenderer},
    };

    const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n";

    fn handler_with(
        source: FakeDiffSource,
    ) -> RenderDiffHandler<FakeDiffSource, InMemoryArtifactStore, StubRenderer, FixedClock> {
        RenderDiffHandler {
            source,
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        }
    }

    fn one_commit() -> Commit {
        Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }
    }

    fn req(source_top: &str, target: DiffTarget) -> RenderDiff {
        RenderDiff {
            cwd: PathBuf::from(source_top),
            store_root: PathBuf::from("/store"),
            target,
            name: None,
            theme: None,
        }
    }

    #[test]
    fn renders_and_stores_an_artifact_with_the_summary_notes() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            committed_at: "2026-07-02".into(),
            ..Default::default()
        };
        let handler = handler_with(source);

        let response =
            send_now(&(), &handler, req("/repo", DiffTarget::Unpushed)).expect("render succeeds");

        assert_eq!(
            response.outcome,
            RenderDiffOutcome::Rendered {
                artifact: PathBuf::from("/store/diffs/fake/artifact.html"),
                reused: false,
            }
        );
        let placed = handler.store.placed.lock().unwrap();
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0.title, "diff");
        assert_eq!(placed[0].0.repo_name, "repo");
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-preview: 1 unpushed commit(s), 1 file(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
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
        let handler = handler_with(source);

        let response =
            send_now(&(), &handler, req("/repo", DiffTarget::Unpushed)).expect("render succeeds");

        assert_eq!(response.outcome, RenderDiffOutcome::Empty);
        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff-preview: 0 unpushed commit(s) — nothing to show (no commits or changes); skipping"
            )]
        );
        assert!(handler.store.placed.lock().unwrap().is_empty());
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
        let handler = handler_with(source);
        handler.store.range_hits.lock().unwrap().insert(
            (DiffKind::TwoDot, "sha-a".to_string(), "sha-b".to_string()),
            PathBuf::from("/store/existing.html"),
        );

        let response = send_now(
            &(),
            &handler,
            req("/repo", DiffTarget::Range("a..b".into())),
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
        assert!(handler.store.placed.lock().unwrap().is_empty());
    }

    #[test]
    fn named_run_skips_the_fast_path_and_overrides_the_title() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            shas: [
                ("a".to_string(), "sha-a".to_string()),
                ("b".to_string(), "sha-b".to_string()),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let handler = handler_with(source);
        // A range hit exists, but a named run must ignore the fast-path entirely.
        handler.store.range_hits.lock().unwrap().insert(
            (DiffKind::TwoDot, "sha-a".to_string(), "sha-b".to_string()),
            PathBuf::from("/store/existing.html"),
        );

        let mut request = req("/repo", DiffTarget::Range("a..b".into()));
        request.name = Some("custom".into());
        let response = send_now(&(), &handler, request).expect("render succeeds");

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered { reused: false, .. }
        ));
        let placed = handler.store.placed.lock().unwrap();
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0.title, "custom");
    }

    #[test]
    fn unpushed_without_upstream_warns_and_falls_back_to_main() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            known_revs: vec!["main".into()],
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };
        let handler = handler_with(source);

        let response =
            send_now(&(), &handler, req("/repo", DiffTarget::Unpushed)).expect("render succeeds");

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
        let handler = handler_with(source);

        let error = send_now(&(), &handler, req("/repo", DiffTarget::Base("nope".into())))
            .expect_err("unknown base errors");

        let RenderDiffError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
