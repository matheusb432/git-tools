//! The `render_diff_subrepos` vertical slice: render every discovered repo into
//! one tabbed artifact, skipping repos whose view is empty (or errors) and
//! reporting the skip count. Repo *discovery* stays cli-side (filesystem
//! walking, not a port); the cli's `gtl diff -r` sends the
//! already-discovered [`RepoRef`]s to the resident daemon over HTTP, which
//! dispatches this request through the daemon mediator.

use std::path::PathBuf;

use domain::diffs::{DiffExclusions, DiffKind, DiffTarget};

use crate::{
    diffs::batch::{RepoRef, dated_title, render_batch},
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render a tabbed diff preview across `repos` under `store_root`.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffSubrepos {
    pub store_root: PathBuf,
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: PathBuf,
    pub target: DiffTarget,
    pub repos: Vec<RepoRef>,
    pub theme: Option<String>,
    pub exclusions: DiffExclusions,
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

/// Everything that can go wrong rendering recursive multi-repo diff previews.
#[derive(Debug, thiserror::Error)]
pub enum RenderDiffSubreposError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders the requested subrepositories through the diff ports.
#[cqrsy::handler(command)]
pub fn execute(
    req: RenderDiffSubrepos,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderDiffSubreposResponse, RenderDiffSubreposError> {
    let RenderDiffSubrepos {
        root,
        store_root,
        repos,
        target,
        theme,
        exclusions,
    } = req;
    let mut notes = Vec::new();
    let batch = render_batch(
        source,
        &target,
        theme.as_deref(),
        &exclusions,
        &repos,
        true,
        &mut notes,
    )?;

    if batch.views.is_empty() {
        notes.push(Note::warn(format!(
            "diff -r: nothing to show across {} repo(s); no preview written",
            repos.len()
        )));
        return Ok(RenderDiffSubreposResponse {
            outcome: RenderDiffSubreposOutcome::Empty,
            notes,
        });
    }

    let title = dated_title(clock, "diff-preview subrepos");
    let html = renderer.build_tabbed_html(&title, &batch.views);
    let meta = ArtifactMeta {
        repo_root: root,
        repo_name: "subrepos".to_string(),
        kind: DiffKind::WorkTree,
        base_sha: String::new(),
        head_sha: String::new(),
        range_label: String::new(),
        head_committed_at: String::new(),
        generated_at: clock.now_iso(),
        title: title.clone(),
        excluded_extensions: Vec::new(),
    };
    let placed = store.place(&store_root, &meta, &html)?;

    notes.push(Note::info(format!(
        "diff -r: {} repo(s)",
        batch.views.len()
    )));
    if batch.skipped > 0 {
        notes.push(Note::warn(format!(
            "diff -r: skipped {} repo(s) with nothing to show",
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::{Commit, DiffExclusions, DiffTarget};

    use super::{RenderDiffSubrepos, RenderDiffSubreposOutcome, RepoRef, execute};
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

    fn req(repos: Vec<RepoRef>) -> RenderDiffSubrepos {
        RenderDiffSubrepos {
            exclusions: DiffExclusions::default(),
            store_root: PathBuf::from("/store"),
            root: PathBuf::from("/scan-root"),
            target: DiffTarget::Unpushed { pinned: None },
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
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: "/repo-a".into(),
            label: "repo-a".into(),
        }];

        let response = execute(
            req(repos),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

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
                Note::info("diff -r: 1 repo(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.title, "2026-07-02 diff-preview subrepos");
        assert_eq!(artifact.meta.repo_name, "subrepos");
    }

    #[test]
    fn all_empty_returns_empty_with_the_summary_warning() {
        let source = FakeDiffSource {
            upstream: Some("origin/main".into()),
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: "/repo-a".into(),
            label: "repo-a".into(),
        }];

        let response = execute(
            req(repos),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(response.outcome, RenderDiffSubreposOutcome::Empty);
        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff -r: nothing to show across 1 repo(s); no preview written"
            )]
        );
    }
}
