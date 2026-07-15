//! The `render_merge_diff` vertical slice: render the diff of everything on
//! the current branch that would merge into `base`, carrying every
//! user-facing message out as [`Note`]s. Mirrors `render_diff`'s shape; the
//! cli's `gtl merge-diff` calls this in-process.

use std::path::{Path, PathBuf};

use domain::diffs::{AppliedExclusions, DiffExclusions, DiffKind};

use crate::{
    diffs::{
        PinnedRange, View,
        range::DiffRanges,
        range_view::{RangePresentation, RangeView, TITLE_MERGE_DIFF},
        util::{DiffData, assemble, exclusion_note, repo_name},
    },
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Falls back to this base when `base` is `None` or blank.
pub const DEFAULT_BASE: &str = "main";

/// Render the merge-diff of the current branch into `base` (default `main`)
/// under `store_root`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMergeDiff {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
    pub base: Option<String>,
    pub exclusions: DiffExclusions,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMergeDiffResponse {
    pub artifact: PathBuf,
    pub reused: bool,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering a merge-diff.
#[derive(Debug, thiserror::Error)]
pub enum RenderMergeDiffError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// The computed merge view plus the range facts the artifact path still needs.
pub(crate) struct MergeViewBuild {
    pub view: View,
    pub top: String,
    pub base: String,
    pub diff_range: String,
}

/// Shared with `compute_merge_diff`: builds the merge [`View`] for the repo at
/// `cwd` into `base` (default [`DEFAULT_BASE`]). `pinned: Some` computes over
/// its resolved SHA range instead, skipping the symbolic `verify_commit`. No
/// HTML, no store.
pub(crate) fn build_merge_view(
    source: &impl DiffSource,
    cwd: &Path,
    base: Option<&str>,
    pinned: Option<&PinnedRange>,
    exclusions: &DiffExclusions,
) -> anyhow::Result<MergeViewBuild> {
    let top = source.top_level(cwd)?;
    let branch = source.current_branch(Path::new(&top))?;
    let repo_name = repo_name(&top);
    let excluded = exclusions.for_project_or_default(&repo_name);
    let base = base
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or(DEFAULT_BASE);

    let (io_ranges, view_ranges) = if let Some(pin) = pinned {
        (
            DiffRanges::exact(pin.git_range()),
            DiffRanges::exact(pin.display_range()),
        )
    } else {
        source.verify_commit(Path::new(&top), base)?;
        let symbolic = DiffRanges::merge(base);
        (symbolic.clone(), symbolic)
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
        theme: None,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    Ok(MergeViewBuild {
        view,
        top,
        base: base.to_string(),
        diff_range: io_ranges.diff,
    })
}

/// Renders a merge diff through the diff ports.
#[cqrsy::handler(command)]
pub fn execute(
    req: RenderMergeDiff,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderMergeDiffResponse, RenderMergeDiffError> {
    let RenderMergeDiff {
        cwd,
        store_root,
        base,
        exclusions,
    } = req;
    let built = build_merge_view(source, &cwd, base.as_deref(), None, &exclusions)?;
    let view = built.view;
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    let html = renderer.build_html(&view);

    let meta = ArtifactMeta {
        repo_root: PathBuf::from(&built.top),
        repo_name: view.repo_name.clone(),
        kind: DiffKind::from_diff_range(&built.diff_range),
        base_sha: source
            .resolve_sha(Path::new(&built.top), &built.base)
            .unwrap_or_default(),
        head_sha: source
            .resolve_sha(Path::new(&built.top), "HEAD")
            .unwrap_or_default(),
        range_label: built.diff_range.clone(),
        head_committed_at: source.committed_at(Path::new(&built.top), "HEAD"),
        generated_at: clock.now_iso(),
        title: TITLE_MERGE_DIFF.to_string(),
        excluded_extensions: exclusions
            .for_project_or_default(&view.repo_name)
            .extensions()
            .to_vec(),
    };
    let placed = store.place(&store_root, &meta, &html)?;

    let mut notes = Vec::new();
    notes.extend(exclusion_note(TITLE_MERGE_DIFF, &view));
    notes.push(Note::info(format!(
        "{TITLE_MERGE_DIFF}: {commit_count} commit{} to merge into {}, {file_count} file{}",
        plural(commit_count),
        built.base,
        plural(file_count),
    )));
    notes.push(Note::info(format!("wrote {}", placed.path.display())));
    Ok(RenderMergeDiffResponse {
        artifact: placed.path,
        reused: placed.reused,
        notes,
    })
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::{Commit, DiffExclusions};

    use super::{RenderMergeDiff, RenderMergeDiffError, execute};
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

    fn one_commit() -> Commit {
        Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }
    }

    fn req(source_top: &str, base: Option<&str>) -> RenderMergeDiff {
        RenderMergeDiff {
            exclusions: DiffExclusions::default(),
            cwd: PathBuf::from(source_top),
            store_root: PathBuf::from("/store"),
            base: base.map(str::to_string),
        }
    }

    #[test]
    fn renders_and_stores_an_artifact_with_the_summary_notes() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo", None),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.artifact,
            PathBuf::from("/store/diffs/fake/artifact.html")
        );
        assert!(!response.reused);
        assert_eq!(
            response.notes,
            vec![
                Note::info("merge-diff: 1 commit to merge into main, 1 file"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "merge-diff");
        assert_eq!(artifact.meta.repo_name, "repo");
    }

    #[test]
    fn blank_base_falls_back_to_default() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec!["main".into()],
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = execute(
            req("/repo", Some("   ")),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.notes[0],
            Note::info("merge-diff: 0 commits to merge into main, 0 files")
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
            req("/repo", Some("nope")),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect_err("unknown base errors");

        let RenderMergeDiffError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
