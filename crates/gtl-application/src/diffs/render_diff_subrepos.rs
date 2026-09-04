//! The `render_diff_subrepos` vertical slice: render every discovered repo into
//! one tabbed artifact, skipping repos whose view is empty (or errors) and
//! reporting the skip count. The server discovers repositories and dispatches
//! this application request with the resulting [`RepoRef`] values.

use gtl_models::{
    artifacts::ArtifactDiffIdentity,
    diffs::ExcludedExtensions,
    paths::{ProjectName, RepositoryRoot},
};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget, DiffTargetRequest, DiffTargetRequestError,
        batch::{RepoRef, dated_title, render_batch},
    },
    ports::{
        ArtifactMeta, ArtifactStore, Clock, GitClient, HtmlRenderer, PlacedArtifact,
        UserSettingsLoadError, UserSettingsReader,
    },
    shared::notes::Note,
};

/// Render a tabbed diff artifact across `repos`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiffSubrepos {
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: RepositoryRoot,
    pub target: DiffTargetRequest,
    pub repos: Vec<RepoRef>,
}

/// The outcome plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffSubreposOk {
    pub outcome: RenderDiffSubreposOutcome,
    pub notes: Vec<Note>,
}

/// What the batch render produced: a stored artifact, or every repo was empty.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderDiffSubreposOutcome {
    /// An artifact was created or reused by the store.
    Rendered(PlacedArtifact),
    /// Every repo's view was empty (or errored); nothing was rendered.
    Empty,
}

/// Everything that can go wrong rendering recursive multi-repo diff artifacts.
#[derive(Debug, thiserror::Error)]
pub enum RenderDiffSubreposError {
    #[error(transparent)]
    InvalidTarget(#[from] DiffTargetRequestError),
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders the requested subrepositories through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderDiffSubrepos,
    app_settings: &impl UserSettingsReader,
    git: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderDiffSubreposOk, RenderDiffSubreposError> {
    let RenderDiffSubrepos {
        root,
        repos,
        target,
    } = req;
    let store_root = super::artifacts::root(&root);
    let target = DiffTarget::try_from(target)?;
    let settings = app_settings.load()?;
    let mut notes = Vec::new();
    let batch = render_batch(git, &target, &settings, &repos, true, &mut notes)?;

    if batch.views.is_empty() {
        notes.push(Note::warn(format!(
            "diff -r: nothing to show across {} repo(s); no artifact written",
            repos.len()
        )));
        return Ok(RenderDiffSubreposOk {
            outcome: RenderDiffSubreposOutcome::Empty,
            notes,
        });
    }

    let generated_at = clock.now().map_err(anyhow::Error::from)?;
    let title = dated_title(&generated_at, "diff-artifact subrepos");
    let render_options = settings.viewer_render_options();
    let theme = settings.theme();
    let html = renderer.build_tabbed_html(&title, &batch.views, render_options, theme)?;
    let meta = ArtifactMeta {
        repo_root: root,
        repo_name: ProjectName::try_from("subrepos").map_err(anyhow::Error::from)?,
        identity: ArtifactDiffIdentity::WorkTree,
        range_label: String::new(),
        head_committed_at: None,
        generated_at,
        title: title.clone(),
        render_options,
        theme,
        excluded_extensions: ExcludedExtensions::default(),
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
    notes.push(Note::info(format!("wrote {}", placed.path().display())));
    Ok(RenderDiffSubreposOk {
        outcome: RenderDiffSubreposOutcome::Rendered(placed),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{
        diffs::DiffExclusions,
        settings::UserSettings,
        viewer::{RenderOptions, Theme},
    };

    use super::{RenderDiffSubrepos, RenderDiffSubreposOutcome, RepoRef};
    use crate::{
        diffs::{DiffTargetRequest, render_diff_subrepos},
        shared::notes::Note,
        utils::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(repos: Vec<RepoRef>) -> RenderDiffSubrepos {
        RenderDiffSubrepos {
            root: crate::utils::repository_root("/scan-root"),
            target: DiffTargetRequest::Unpushed,
            repos,
        }
    }

    #[test]
    fn renders_a_tabbed_artifact_and_reports_the_skip_count() {
        let source = FakeGitClient {
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: crate::utils::repository_root("/repo-a"),
            label: crate::utils::project_name("repo-a"),
        }];

        let response = render_diff_subrepos::execute(
            req(repos),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
        )
        .unwrap();

        assert_eq!(
            response.outcome,
            RenderDiffSubreposOutcome::Rendered(crate::ports::PlacedArtifact::Created {
                path: crate::utils::absolute_file_path("/scan-root/.artifacts/gtl/artifact.html",),
            })
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
            .unwrap();
        assert_eq!(artifact.meta.title, "2026-07-02 diff-artifact subrepos");
        assert_eq!(
            artifact.meta.repo_name,
            crate::utils::project_name("subrepos")
        );
    }

    #[test]
    fn all_empty_returns_empty_with_the_summary_warning() {
        let source = FakeGitClient {
            upstream: Some("origin/main".into()),
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: crate::utils::repository_root("/repo-a"),
            label: crate::utils::project_name("repo-a"),
        }];

        let response = render_diff_subrepos::execute(
            req(repos),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
        )
        .unwrap();

        assert_eq!(response.outcome, RenderDiffSubreposOutcome::Empty);
        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff -r: nothing to show across 1 repo(s); no artifact written"
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
        let source = FakeGitClient {
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: TWO_FILE_DIFF.into(),
            ..Default::default()
        };
        let app_settings = FixedUserSettingsStore::new(UserSettings::new(
            Some(Theme::Noir),
            RenderOptions::DEFAULT,
            true,
            DiffExclusions::new([(crate::utils::project_name("repo-a"), vec!["md"])], None),
            gtl_models::settings::PushAllExclusions::default(),
        ));
        let store = InMemoryArtifactStore::default();

        render_diff_subrepos::execute(
            RenderDiffSubrepos {
                root: crate::utils::repository_root("/scan-root"),
                target: DiffTargetRequest::Unpushed,
                repos: vec![RepoRef {
                    top: crate::utils::repository_root("/repo-a"),
                    label: crate::utils::project_name("repo-a"),
                }],
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
        )
        .unwrap();

        let artifact = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert!(artifact.html.contains("repo-a:noir:unified:compact:1"));
    }
}
