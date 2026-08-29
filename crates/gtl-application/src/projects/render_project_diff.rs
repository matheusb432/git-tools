//! The `render_project_diff` vertical slice: render every already-filtered repo
//! (upstream present, unpushed count > 0 -- filtering stays cli-side) into one
//! tabbed artifact. Unlike `render_diff_subrepos`, this never skips an empty
//! view and propagates a build error instead of swallowing it as a skip --
//! matches `run_project_all`'s current no-skip behavior exactly.

use gtl_models::{
    artifacts::ArtifactDiffIdentity,
    diffs::ExcludedExtensions,
    paths::{ProjectName, RepositoryRoot},
};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget, artifacts,
        batch::{RepoRef, dated_title, render_batch},
    },
    ports::{
        ArtifactMeta, ArtifactStore, Clock, GitClient, HtmlRenderer, PlacedArtifact,
        UserSettingsLoadError, UserSettingsStore,
    },
    shared::notes::Note,
};

/// Render a tabbed diff artifact across every repo in `repos`, which the caller
/// already filtered to upstream-present and unpushed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderProjectDiff {
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: RepositoryRoot,
    pub repos: Vec<RepoRef>,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderProjectDiffOk {
    pub placement: PlacedArtifact,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering diff-all.
#[derive(Debug, thiserror::Error)]
pub enum RenderProjectDiffError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders every requested repository through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderProjectDiff,
    app_settings: &impl UserSettingsStore,
    source: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderProjectDiffOk, RenderProjectDiffError> {
    let RenderProjectDiff { root, repos } = req;
    let store_root = artifacts::root(&root);
    let settings = app_settings.load()?;
    let mut notes = Vec::new();
    let batch = render_batch(
        source,
        &DiffTarget::Unpushed { pinned: None },
        &settings,
        &repos,
        false,
        &mut notes,
    )?;

    let generated_at = clock.now().map_err(anyhow::Error::from)?;
    let title = dated_title(&generated_at, "diff-artifact all");
    let render_options = settings.viewer_render_options();
    let theme = settings.theme();
    let html = renderer.build_tabbed_html(&title, &batch.views, render_options, theme)?;
    let meta = ArtifactMeta {
        repo_root: root,
        repo_name: ProjectName::try_from("all").map_err(anyhow::Error::from)?,
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
        "diff-all: {} repo(s)",
        batch.views.len()
    )));
    notes.push(Note::info(format!("wrote {}", placed.path().display())));
    Ok(RenderProjectDiffOk {
        placement: placed,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{
        artifacts::ArtifactDiffIdentity,
        diffs::DiffExclusions,
        settings::UserSettings,
        viewer::{RenderOptions, Theme},
    };

    use super::{RenderProjectDiff, RepoRef};
    use crate::{
        projects::render_project_diff,
        shared::notes::Note,
        utils::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore, RepoOverride,
            SequenceUserSettingsStore, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn settings(theme: Theme, exclusions: DiffExclusions) -> UserSettings {
        UserSettings::new(
            Some(theme),
            RenderOptions::DEFAULT,
            true,
            exclusions,
            gtl_models::settings::PushAllExclusions::default(),
        )
    }

    fn req(repos: Vec<RepoRef>) -> RenderProjectDiff {
        RenderProjectDiff {
            root: crate::utils::repository_root("/scan-root"),
            repos,
        }
    }

    #[test]
    fn renders_every_repo_without_skipping_empties() {
        let source = FakeGitClient {
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![
            RepoRef {
                top: crate::utils::repository_root("/repo-a"),
                label: crate::utils::project_name("repo-a"),
            },
            RepoRef {
                top: crate::utils::repository_root("/repo-b"),
                label: crate::utils::project_name("repo-b"),
            },
        ];

        let response = render_project_diff::execute(
            req(repos),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
        )
        .unwrap();

        assert_eq!(
            response.placement.path().as_path(),
            PathBuf::from("/scan-root/.artifacts/gtl/artifact.html")
        );
        assert!(!response.placement.is_reused());
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-all: 2 repo(s)"),
                Note::info("wrote /scan-root/.artifacts/gtl/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert_eq!(artifact.meta.repo_name, crate::utils::project_name("all"));
        assert_eq!(artifact.meta.identity, ArtifactDiffIdentity::WorkTree);
    }

    #[test]
    fn a_build_error_propagates_instead_of_being_skipped() {
        let source = FakeGitClient {
            upstream: None,
            known_revs: vec![],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: crate::utils::repository_root("/repo"),
            label: crate::utils::project_name("repo"),
        }];

        let result = render_project_diff::execute(
            req(repos),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
        );

        assert!(result.is_err());
    }

    #[test]
    fn batch_reuses_one_snapshot_and_the_next_operation_observes_a_change() {
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
        let mut source = FakeGitClient {
            upstream: Some("origin/main".into()),
            ..Default::default()
        };
        for top in ["/repo-a", "/repo-b"] {
            source.per_repo.insert(
                top.into(),
                RepoOverride {
                    commits: vec![commit("abc1234")],
                    diff_output: TWO_FILE_DIFF.into(),
                },
            );
        }
        let app_settings = SequenceUserSettingsStore::new([
            settings(
                Theme::Hearth,
                DiffExclusions::new([(crate::utils::project_name("repo-a"), vec!["md"])], None),
            ),
            settings(
                Theme::Light,
                DiffExclusions::new([(crate::utils::project_name("repo-b"), vec!["txt"])], None),
            ),
        ]);
        let store = InMemoryArtifactStore::default();
        let repos = vec![
            RepoRef {
                top: crate::utils::repository_root("/repo-a"),
                label: crate::utils::project_name("repo-a"),
            },
            RepoRef {
                top: crate::utils::repository_root("/repo-b"),
                label: crate::utils::project_name("repo-b"),
            },
        ];

        render_project_diff::execute(
            RenderProjectDiff {
                root: crate::utils::repository_root("/scan-root"),
                repos: repos.clone(),
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
        )
        .unwrap();
        let first_html = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .unwrap()
            .html;

        render_project_diff::execute(
            RenderProjectDiff {
                root: crate::utils::repository_root("/scan-root"),
                repos,
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
        )
        .unwrap();
        let second_html = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .unwrap()
            .html;

        assert!(
            first_html.contains("repo-a:hearth:unified:compact:1|repo-b:hearth:unified:compact:2")
        );
        assert!(
            second_html.contains("repo-a:light:unified:compact:2|repo-b:light:unified:compact:1")
        );
    }
}
