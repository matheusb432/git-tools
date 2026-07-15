//! The `render_diff_all` vertical slice: render every already-filtered repo
//! (upstream present, unpushed count > 0 -- filtering stays cli-side) into one
//! tabbed artifact. Unlike `render_diff_subrepos`, this never skips an empty
//! view and propagates a build error instead of swallowing it as a skip --
//! matches `run_managed_all`'s current no-skip behavior exactly.

use std::path::PathBuf;

use domain::diffs::{DiffExclusions, DiffKind};

use crate::{
    diffs::{
        DiffTarget,
        batch::{RepoRef, dated_title, render_batch},
    },
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render a tabbed diff preview across every repo in `repos` (already filtered
/// by the caller to upstream-present + unpushed > 0) under `store_root`.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffAll {
    pub store_root: PathBuf,
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: PathBuf,
    pub repos: Vec<RepoRef>,
    pub theme: Option<String>,
    pub exclusions: DiffExclusions,
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

/// Renders every requested repository through the diff ports.
#[cqrsy::handler(command)]
pub fn execute(
    req: RenderDiffAll,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderDiffAllResponse, RenderDiffAllError> {
    let RenderDiffAll {
        root,
        store_root,
        repos,
        theme,
        exclusions,
    } = req;
    let mut notes = Vec::new();
    let batch = render_batch(
        source,
        &DiffTarget::Unpushed { pinned: None },
        theme.as_deref(),
        &exclusions,
        &repos,
        false,
        &mut notes,
    )?;

    let title = dated_title(clock, "diff-preview all");
    let html = renderer.build_tabbed_html(&title, &batch.views);
    let meta = ArtifactMeta {
        repo_root: root,
        repo_name: "all".to_string(),
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::diffs::{Commit, DiffExclusions, DiffKind};

    use super::{RenderDiffAll, RepoRef, execute};
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

    fn req(repos: Vec<RepoRef>) -> RenderDiffAll {
        RenderDiffAll {
            exclusions: DiffExclusions::default(),
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
        let store = InMemoryArtifactStore::default();
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

        let response = execute(
            req(repos),
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
                Note::info("diff-all: 2 repo(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.repo_name, "all");
        assert_eq!(artifact.meta.kind, DiffKind::WorkTree);
    }

    #[test]
    fn a_build_error_propagates_instead_of_being_skipped() {
        let source = FakeDiffSource {
            upstream: None,
            known_revs: vec![],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: "/repo".into(),
            label: "repo".into(),
        }];

        let result = execute(
            req(repos),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        );

        assert!(result.is_err());
    }
}
