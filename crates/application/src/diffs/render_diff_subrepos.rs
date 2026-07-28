//! The `render_diff_subrepos` vertical slice: render every discovered repo into
//! one tabbed artifact, skipping repos whose view is empty (or errors) and
//! reporting the skip count. Repo *discovery* stays cli-side (filesystem
//! walking, not a port); the cli's `gtl diff -r` sends the
//! already-discovered [`RepoRef`]s to the resident daemon over HTTP, which
//! dispatches this application request directly.

use std::path::PathBuf;

use domain::diffs::DiffKind;
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget, DiffTargetRequest, DiffTargetRequestError,
        batch::{RepoRef, dated_title, render_batch},
    },
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer, UserSettingsStore},
    shared::notes::Note,
};

/// Render a tabbed diff preview across `repos`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiffSubrepos {
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: PathBuf,
    pub target: DiffTargetRequest,
    pub repos: Vec<RepoRef>,
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
    InvalidTarget(#[from] DiffTargetRequestError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders the requested subrepositories through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderDiffSubrepos,
    app_settings: &impl UserSettingsStore,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderDiffSubreposResponse, RenderDiffSubreposError> {
    let RenderDiffSubrepos {
        root,
        repos,
        target,
    } = req;
    let store_root = super::artifacts::root(&root);
    let target = DiffTarget::try_from(target)?;
    let settings = app_settings.load();
    let mut notes = Vec::new();
    let batch = render_batch(source, &target, &settings, &repos, true, &mut notes)?;

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
    let render_options = settings.viewer_render_options();
    let theme = settings.theme().map(str::to_owned);
    let html = renderer.build_tabbed_html(&title, &batch.views, render_options, theme.as_deref());
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
        render_options,
        theme,
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

    use domain::diffs::DiffExclusions;

    use super::{RenderDiffSubrepos, RenderDiffSubreposOutcome, RepoRef, execute};
    use crate::{
        diffs::DiffTargetRequest,
        ports::AppSettings,
        shared::notes::Note,
        testing::{
            FakeDiffSource, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore,
            StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(repos: Vec<RepoRef>) -> RenderDiffSubrepos {
        RenderDiffSubrepos {
            root: PathBuf::from("/scan-root"),
            target: DiffTargetRequest::Unpushed,
            repos,
        }
    }

    #[test]
    fn renders_a_tabbed_artifact_and_reports_the_skip_count() {
        let source = FakeDiffSource {
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: "/repo-a".into(),
            label: "repo-a".into(),
        }];

        let response = execute(
            req(repos),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.outcome,
            RenderDiffSubreposOutcome::Rendered {
                artifact: PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"),
                reused: false,
            }
        );
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff -r: 1 repo(s)"),
                Note::info("wrote /scan-root/.artifacts/gtl/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
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
            &FixedUserSettingsStore::default(),
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

    #[test]
    fn render_uses_the_settings_snapshot_for_the_batch() {
        const TWO_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1 @@\n\
-old\n\
+new\n\
diff --git a/notes.md b/notes.md\n\
--- a/notes.md\n\
+++ b/notes.md\n\
@@ -1 +1 @@\n\
-plan\n\
+more plan\n";
        let source = FakeDiffSource {
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: TWO_FILE_DIFF.into(),
            ..Default::default()
        };
        let app_settings = FixedUserSettingsStore::new(AppSettings::new(
            Some("night".into()),
            true,
            DiffExclusions::new([("repo-a".to_string(), vec!["md"])], None),
        ));
        let store = InMemoryArtifactStore::default();

        execute(
            RenderDiffSubrepos {
                root: PathBuf::from("/scan-root"),
                target: DiffTargetRequest::Unpushed,
                repos: vec![RepoRef {
                    top: "/repo-a".into(),
                    label: "repo-a".into(),
                }],
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        let artifact = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .expect("artifact persisted");
        assert!(artifact.html.contains("repo-a:night:unified:compact:1"));
    }
}
