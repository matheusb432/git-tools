//! The `render_diff_subrepos` vertical slice: render every discovered repo into
//! one tabbed artifact, skipping repos whose view is empty (or errors) and
//! reporting the skip count. Repo *discovery* stays cli-side (filesystem
//! walking, not a port); the cli's `gtl diff subrepos` calls this in-process
//! with the already-discovered [`RepoRef`]s.

use std::path::PathBuf;

use domain::diffs::{DiffKind, DiffTarget};

use crate::{
    diffs::batch::{RepoRef, dated_title, render_batch},
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render a tabbed diff preview across `repos` under `store_root`.
#[derive(Debug, Clone, PartialEq, cqrs::Request)]
#[request(response = RenderDiffSubreposResponse, error = RenderDiffSubreposError)]
pub struct RenderDiffSubrepos {
    pub store_root: PathBuf,
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: PathBuf,
    pub target: DiffTarget,
    pub repos: Vec<RepoRef>,
    pub theme: Option<String>,
}

/// The outcome plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffSubreposResponse {
    pub outcome: RenderDiffSubreposOutcome,
    pub notes: Vec<Note>,
}

/// What the batch render produced: a stored artifact, or every repo was empty.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderDiffSubreposOutcome {
    /// An artifact landed at `artifact`; `reused` when an identical one already existed.
    Rendered { artifact: PathBuf, reused: bool },
    /// Every repo's view was empty (or errored); nothing was rendered.
    Empty,
}

/// Everything that can go wrong rendering diff-subrepos.
#[derive(Debug, thiserror::Error)]
pub enum RenderDiffSubreposError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Handles [`RenderDiffSubrepos`] by driving the diff engine through its ports.
#[derive(Clone)]
pub struct RenderDiffSubreposHandler<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> {
    pub source: S,
    pub store: A,
    pub renderer: R,
    pub clock: C,
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock>
    RenderDiffSubreposHandler<S, A, R, C>
{
    /// Synchronous core — the daemon's async handler and the cli's in-process
    /// path both delegate here.
    pub fn execute(
        &self,
        req: &RenderDiffSubrepos,
    ) -> Result<RenderDiffSubreposResponse, RenderDiffSubreposError> {
        Ok(self.render(req)?)
    }

    fn render(&self, req: &RenderDiffSubrepos) -> anyhow::Result<RenderDiffSubreposResponse> {
        let mut notes = Vec::new();
        let batch = render_batch(
            &self.source,
            &req.target,
            req.theme.clone(),
            &req.repos,
            true,
            &mut notes,
        )?;

        if batch.views.is_empty() {
            notes.push(Note::warn(format!(
                "diff subrepos: nothing to show across {} repo(s); no preview written",
                req.repos.len()
            )));
            return Ok(RenderDiffSubreposResponse {
                outcome: RenderDiffSubreposOutcome::Empty,
                notes,
            });
        }

        let title = dated_title(&self.clock, "diff-preview subrepos");
        let html = self.renderer.build_tabbed_html(&title, &batch.views);
        let meta = ArtifactMeta {
            repo_root: req.root.clone(),
            repo_name: "subrepos".to_string(),
            kind: DiffKind::WorkTree,
            base_sha: String::new(),
            head_sha: String::new(),
            range_label: String::new(),
            head_committed_at: String::new(),
            generated_at: self.clock.now_iso(),
            title: title.clone(),
        };
        let placed = self.store.place(&req.store_root, &meta, &html)?;

        notes.push(Note::info(format!(
            "diff subrepos: {} repo(s)",
            batch.views.len()
        )));
        if batch.skipped > 0 {
            notes.push(Note::warn(format!(
                "diff subrepos: skipped {} repo(s) with nothing to show",
                batch.skipped
            )));
        }
        notes.push(Note::info(format!("wrote {}", placed.path.display())));
        Ok(RenderDiffSubreposResponse {
            outcome: RenderDiffSubreposOutcome::Rendered {
                artifact: placed.path,
                reused: placed.reused,
            },
            notes,
        })
    }
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock>
    cqrs::RequestHandler<RenderDiffSubrepos> for RenderDiffSubreposHandler<S, A, R, C>
{
    async fn handle(
        &self,
        req: RenderDiffSubrepos,
    ) -> Result<RenderDiffSubreposResponse, RenderDiffSubreposError> {
        self.execute(&req)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::{Commit, DiffTarget};

    use super::{
        RenderDiffSubrepos, RenderDiffSubreposHandler, RenderDiffSubreposOutcome, RepoRef,
    };
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
    ) -> RenderDiffSubreposHandler<FakeDiffSource, InMemoryArtifactStore, StubRenderer, FixedClock>
    {
        RenderDiffSubreposHandler {
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

    fn req(repos: Vec<RepoRef>) -> RenderDiffSubrepos {
        RenderDiffSubrepos {
            store_root: PathBuf::from("/store"),
            root: PathBuf::from("/scan-root"),
            target: DiffTarget::Unpushed,
            repos,
            theme: None,
        }
    }

    #[test]
    fn renders_a_tabbed_artifact_and_reports_the_skip_count() {
        let source = FakeDiffSource {
            upstream: Some("origin/main".into()),
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };
        let handler = handler_with(source);
        let repos = vec![RepoRef {
            top: "/repo-a".into(),
            label: "repo-a".into(),
        }];

        let response = handler.execute(&req(repos)).expect("render succeeds");

        assert_eq!(
            response.outcome,
            RenderDiffSubreposOutcome::Rendered {
                artifact: PathBuf::from("/store/diffs/fake/artifact.html"),
                reused: false,
            }
        );
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff subrepos: 1 repo(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let placed = handler.store.placed.lock().unwrap();
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0.title, "2026-07-02 diff-preview subrepos");
        assert_eq!(placed[0].0.repo_name, "subrepos");
    }

    #[test]
    fn all_empty_returns_empty_with_the_summary_warning() {
        let source = FakeDiffSource {
            upstream: Some("origin/main".into()),
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let handler = handler_with(source);
        let repos = vec![RepoRef {
            top: "/repo-a".into(),
            label: "repo-a".into(),
        }];

        let response = handler.execute(&req(repos)).expect("render succeeds");

        assert_eq!(response.outcome, RenderDiffSubreposOutcome::Empty);
        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff subrepos: nothing to show across 1 repo(s); no preview written"
            )]
        );
        assert!(handler.store.placed.lock().unwrap().is_empty());
    }
}
