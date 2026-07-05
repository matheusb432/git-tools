//! The `render_diff_all` vertical slice: render every already-filtered repo
//! (upstream present, unpushed count > 0 -- filtering stays cli-side) into one
//! tabbed artifact. Unlike `render_diff_subrepos`, this never skips an empty
//! view and propagates a build error instead of swallowing it as a skip --
//! matches `run_managed_all`'s current no-skip behavior exactly.

use std::path::PathBuf;

use domain::diffs::{DiffKind, DiffTarget};

use crate::{
    diffs::batch::{RepoRef, dated_title, render_batch},
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render a tabbed diff preview across every repo in `repos` (already filtered
/// by the caller to upstream-present + unpushed > 0) under `store_root`.
#[derive(Debug, Clone, PartialEq, cqrsy::Command)]
#[command(out = RenderDiffAllResponse, err = RenderDiffAllError)]
pub struct RenderDiffAll {
    pub store_root: PathBuf,
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: PathBuf,
    pub repos: Vec<RepoRef>,
    pub theme: Option<String>,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffAllResponse {
    pub artifact: PathBuf,
    pub reused: bool,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering diff-all.
#[derive(Debug, thiserror::Error)]
pub enum RenderDiffAllError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Handles [`RenderDiffAll`] by driving the diff engine through its ports.
#[derive(Clone)]
pub struct RenderDiffAllHandler<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> {
    pub source: S,
    pub store: A,
    pub renderer: R,
    pub clock: C,
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> cqrsy::Handler<RenderDiffAll>
    for RenderDiffAllHandler<S, A, R, C>
{
    async fn handle(
        &self,
        req: RenderDiffAll,
    ) -> Result<RenderDiffAllResponse, RenderDiffAllError> {
        let mut notes = Vec::new();
        let batch = render_batch(
            &self.source,
            &DiffTarget::Unpushed,
            req.theme.clone(),
            &req.repos,
            false,
            &mut notes,
        )?;

        let title = dated_title(&self.clock, "diff-preview all");
        let html = self.renderer.build_tabbed_html(&title, &batch.views);
        let meta = ArtifactMeta {
            repo_root: req.root.clone(),
            repo_name: "all".to_string(),
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
            "diff-all: {} repo(s)",
            batch.views.len()
        )));
        notes.push(Note::info(format!("wrote {}", placed.path.display())));
        Ok(RenderDiffAllResponse {
            artifact: placed.path,
            reused: placed.reused,
            notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cqrsy::send_now;
    use domain::diffs::{Commit, DiffKind};

    use super::{RenderDiffAll, RenderDiffAllHandler, RepoRef};
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
    ) -> RenderDiffAllHandler<FakeDiffSource, InMemoryArtifactStore, StubRenderer, FixedClock> {
        RenderDiffAllHandler {
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

    fn req(repos: Vec<RepoRef>) -> RenderDiffAll {
        RenderDiffAll {
            store_root: PathBuf::from("/store"),
            root: PathBuf::from("/scan-root"),
            repos,
            theme: None,
        }
    }

    #[test]
    fn renders_every_repo_without_skipping_empties() {
        let source = FakeDiffSource {
            upstream: Some("origin/main".into()),
            commits: vec![one_commit()],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };
        let handler = handler_with(source);
        let repos = vec![
            RepoRef {
                top: "/repo-a".into(),
                label: "repo-a".into(),
            },
            RepoRef {
                top: "/repo-b".into(),
                label: "repo-b".into(),
            },
        ];

        let response = send_now(&(), &handler, req(repos)).expect("render succeeds");

        assert_eq!(
            response.artifact,
            PathBuf::from("/store/diffs/fake/artifact.html")
        );
        assert!(!response.reused);
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-all: 2 repo(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let placed = handler.store.placed.lock().unwrap();
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0.repo_name, "all");
        assert_eq!(placed[0].0.kind, DiffKind::WorkTree);
    }

    #[test]
    fn a_build_error_propagates_instead_of_being_skipped() {
        let source = FakeDiffSource {
            upstream: None,
            known_revs: vec![],
            ..Default::default()
        };
        let handler = handler_with(source);
        let repos = vec![RepoRef {
            top: "/repo".into(),
            label: "repo".into(),
        }];

        let result = send_now(&(), &handler, req(repos));

        assert!(result.is_err());
        assert!(handler.store.placed.lock().unwrap().is_empty());
    }
}
