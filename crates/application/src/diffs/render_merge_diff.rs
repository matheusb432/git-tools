//! The `render_merge_diff` vertical slice: render the diff of everything on
//! the current branch that would merge into `base`, carrying every
//! user-facing message out as [`Note`]s. Mirrors `render_diff`'s shape; the
//! cli's `gtl merge-diff` calls this in-process.

use std::path::{Path, PathBuf};

use domain::diffs::{DiffKind, Mode, View, ranges};

use crate::{
    diffs::util::{DiffData, assemble},
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Falls back to this base when `base` is `None` or blank.
pub const DEFAULT_BASE: &str = "main";

/// Render the merge-diff of the current branch into `base` (default `main`)
/// under `store_root`, resolving the repo from `cwd`.
#[derive(Debug, Clone, PartialEq, cqrs::Request)]
#[request(response = RenderMergeDiffResponse, error = RenderMergeDiffError)]
pub struct RenderMergeDiff {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
    pub base: Option<String>,
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

/// Handles [`RenderMergeDiff`] by driving the diff engine through its ports.
#[derive(Clone)]
pub struct RenderMergeDiffHandler<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> {
    pub source: S,
    pub store: A,
    pub renderer: R,
    pub clock: C,
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock>
    RenderMergeDiffHandler<S, A, R, C>
{
    /// Synchronous core — the daemon's async handler and the cli's in-process
    /// path both delegate here.
    pub fn execute(
        &self,
        req: &RenderMergeDiff,
    ) -> Result<RenderMergeDiffResponse, RenderMergeDiffError> {
        Ok(self.render(req)?)
    }

    fn render(&self, req: &RenderMergeDiff) -> anyhow::Result<RenderMergeDiffResponse> {
        let top = self.source.top_level(&req.cwd)?;
        let branch = self.source.current_branch(Path::new(&top))?;
        let repo_name = repo_name(&top);
        let base = req
            .base
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .unwrap_or(DEFAULT_BASE);

        self.source.verify_commit(Path::new(&top), base)?;
        let ranges = ranges(base, Mode::Merge);
        let DiffData { commits, files } = assemble(
            &self.source,
            Path::new(&top),
            &ranges.diff_args,
            &ranges.diff_range,
            &ranges.log_range,
        )?;

        let view = View {
            repo_name: repo_name.clone(),
            repo_root: top.clone(),
            branch,
            upstream: base.to_string(),
            title: ranges.title,
            cmd: ranges.cmd,
            commits_label: ranges.commits_label,
            foot: ranges.foot,
            commits,
            files,
            theme: None,
        };

        let commit_count = view.commits.len();
        let file_count = view.files.len();
        let html = self.renderer.build_html(&view);

        let meta = ArtifactMeta {
            repo_root: PathBuf::from(&top),
            repo_name: repo_name.clone(),
            kind: DiffKind::from_diff_range(&ranges.diff_range),
            base_sha: self
                .source
                .resolve_sha(Path::new(&top), base)
                .unwrap_or_default(),
            head_sha: self
                .source
                .resolve_sha(Path::new(&top), "HEAD")
                .unwrap_or_default(),
            range_label: ranges.diff_range.clone(),
            head_committed_at: self.source.committed_at(Path::new(&top), "HEAD"),
            generated_at: self.clock.now_iso(),
            title: "merge-diff".to_string(),
        };
        let placed = self.store.place(&req.store_root, &meta, &html)?;

        let notes = vec![
            Note::info(format!(
                "merge-diff: {commit_count} commit{} to merge into {base}, {file_count} file{}",
                plural(commit_count),
                plural(file_count),
            )),
            Note::info(format!("wrote {}", placed.path.display())),
        ];
        Ok(RenderMergeDiffResponse {
            artifact: placed.path,
            reused: placed.reused,
            notes,
        })
    }
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock>
    cqrs::RequestHandler<RenderMergeDiff> for RenderMergeDiffHandler<S, A, R, C>
{
    async fn handle(
        &self,
        req: RenderMergeDiff,
    ) -> Result<RenderMergeDiffResponse, RenderMergeDiffError> {
        self.execute(&req)
    }
}

fn repo_name(top: &str) -> String {
    Path::new(top)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::Commit;

    use super::{RenderMergeDiff, RenderMergeDiffError, RenderMergeDiffHandler};
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
    ) -> RenderMergeDiffHandler<FakeDiffSource, InMemoryArtifactStore, StubRenderer, FixedClock>
    {
        RenderMergeDiffHandler {
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

    fn req(source_top: &str, base: Option<&str>) -> RenderMergeDiff {
        RenderMergeDiff {
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
        let handler = handler_with(source);

        let response = handler
            .execute(&req("/repo", None))
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
        let placed = handler.store.placed.lock().unwrap();
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0.title, "merge-diff");
        assert_eq!(placed[0].0.repo_name, "repo");
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
        let handler = handler_with(source);

        let response = handler
            .execute(&req("/repo", Some("   ")))
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
        let handler = handler_with(source);

        let error = handler
            .execute(&req("/repo", Some("nope")))
            .expect_err("unknown base errors");

        let RenderMergeDiffError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
