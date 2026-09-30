//! Renders selected project comparisons into one artifact, reporting unavailable bases.

use gtl_models::{
    artifacts::ArtifactDiffIdentity, diffs::ExtensionFilter, failure::ErrorMeta,
    paths::RepositoryRoot,
};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget, artifacts,
        batch::{RepoRef, dated_title, render_batch},
    },
    ports::{
        ArtifactMeta, ArtifactStore, Clock, GitClient, HtmlRenderer, PlacedArtifact,
        RepositoryPreferenceReader, UserSettingsLoadError, UserSettingsReader,
    },
    shared::notes::Note,
};

/// Renders repositories selected for commits ahead of their comparison base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderProjectDiff {
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: RepositoryRoot,
    pub repos: Vec<RepoRef>,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderProjectDiffOk {
    pub placement: Option<PlacedArtifact>,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering diff-all.
#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum RenderProjectDiffError {
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// Renders every requested repository through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderProjectDiff,
    app_settings: &impl UserSettingsReader,
    git: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
    preferences: &impl RepositoryPreferenceReader,
) -> Result<RenderProjectDiffOk, RenderProjectDiffError> {
    let RenderProjectDiff { root, repos } = req;
    let store_root = artifacts::root(&root);
    let settings = app_settings.load()?;
    let mut notes = Vec::new();
    let batch = render_batch(
        git,
        &DiffTarget::Unpushed { pinned: None },
        preferences,
        &repos,
        false,
        &mut notes,
        preferences,
    )?;

    if batch.views.is_empty() {
        return Ok(RenderProjectDiffOk {
            placement: None,
            notes,
        });
    }

    let generated_at = clock.now().map_err(anyhow::Error::from)?;
    let title = dated_title(&generated_at, "diff-artifact all");
    let render_options = settings
        .viewer_render_options()
        .with_layout(gtl_models::viewer::DiffLayout::Unified);
    let theme = settings.theme();
    let language = settings.language();
    let html = renderer.build_tabbed_html(&title, &batch.views, render_options, theme, language)?;
    let meta = ArtifactMeta {
        repo_root: root,
        identity: ArtifactDiffIdentity::WorkTree,
        generated_at,
        render_options,
        theme,
        language,
        extension_filter: ExtensionFilter::default(),
    };
    let placed = store.place(&store_root, &meta, &html)?;

    notes.push(Note::info(format!(
        "diff-all: {} repo(s)",
        batch.views.len()
    )));
    notes.push(Note::info(format!("wrote {}", placed.path().display())));
    Ok(RenderProjectDiffOk {
        placement: Some(placed),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{
        artifacts::ArtifactDiffIdentity, diffs::ExtensionFilter, settings::UserSettings,
        viewer::Theme,
    };

    use super::{RenderProjectDiff, RepoRef};
    use crate::{
        ports::ExtensionFilterWriter as _,
        projects::render_project_diff,
        shared::notes::Note,
        utils::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore, RepoOverride,
            SavedExtensionFilters, SavedRepositoryPreferences, SequenceUserSettingsStore,
            StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
            hiding_extensions, repository_root,
        },
    };

    fn settings(theme: Theme) -> UserSettings {
        UserSettings::default().with_theme(Some(theme))
    }

    fn req(repos: Vec<RepoRef>) -> RenderProjectDiff {
        RenderProjectDiff {
            root: crate::utils::repository_root("//fixture.invalid/repositories/scan-root"),
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
                top: crate::utils::repository_root("//fixture.invalid/repositories/repo-a"),
                label: crate::utils::project_name("repo-a"),
            },
            RepoRef {
                top: crate::utils::repository_root("//fixture.invalid/repositories/repo-b"),
                label: crate::utils::project_name("repo-b"),
            },
        ];

        let filters = SavedRepositoryPreferences::default();
        let response = render_project_diff::execute(
            req(repos),
            &FixedUserSettingsStore::new(
                UserSettings::default().with_viewer_render_options(
                    gtl_models::viewer::RenderOptions::DEFAULT
                        .with_layout(gtl_models::viewer::DiffLayout::Split),
                ),
            ),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &filters,
        )
        .unwrap();

        assert_eq!(
            response.placement.as_ref().unwrap().path().as_path(),
            PathBuf::from("//fixture.invalid/repositories/scan-root/.artifacts/gtl/artifact.html")
        );
        assert!(!response.placement.as_ref().unwrap().is_reused());
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-all: 2 repo(s)"),
                Note::info(format!(
                    "wrote {}",
                    PathBuf::from("//fixture.invalid/repositories/scan-root")
                        .join(".artifacts")
                        .join("gtl")
                        .join("artifact.html")
                        .display()
                )),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from(
                "//fixture.invalid/repositories/scan-root/.artifacts/gtl/artifact.html",
            ))
            .unwrap();
        assert_eq!(artifact.meta.identity, ArtifactDiffIdentity::WorkTree);
        assert_eq!(
            artifact.meta.render_options.layout(),
            gtl_models::viewer::DiffLayout::Unified
        );
    }

    #[test]
    fn unavailable_comparisons_produce_warnings_without_an_empty_artifact() {
        let source = FakeGitClient {
            upstream: None,
            known_revs: vec![],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let repos = vec![RepoRef {
            top: crate::utils::repository_root("//fixture.invalid/repositories/repo"),
            label: crate::utils::project_name("repo"),
        }];

        let filters = SavedRepositoryPreferences::default();
        let result = render_project_diff::execute(
            req(repos),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &filters,
        );

        let result = result.unwrap();
        assert!(result.placement.is_none());
        assert!(result.notes[0].text.contains("Local comparison branch"));
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
        for top in [
            "//fixture.invalid/repositories/repo-a",
            "//fixture.invalid/repositories/repo-b",
        ] {
            source.per_repo.insert(
                top.into(),
                RepoOverride {
                    commits: vec![commit("abc1234")],
                    diff_output: TWO_FILE_DIFF.into(),
                },
            );
        }
        let app_settings =
            SequenceUserSettingsStore::new([settings(Theme::Mirage), settings(Theme::Glacier)]);
        let filters = SavedRepositoryPreferences::from(SavedExtensionFilters::new([(
            repository_root("//fixture.invalid/repositories/repo-a"),
            hiding_extensions(&["md"]),
        )]));
        let store = InMemoryArtifactStore::default();
        let artifact_path = PathBuf::from("//fixture.invalid/repositories/scan-root")
            .join(".artifacts")
            .join("gtl")
            .join("artifact.html");
        let repos = vec![
            RepoRef {
                top: crate::utils::repository_root("//fixture.invalid/repositories/repo-a"),
                label: crate::utils::project_name("repo-a"),
            },
            RepoRef {
                top: crate::utils::repository_root("//fixture.invalid/repositories/repo-b"),
                label: crate::utils::project_name("repo-b"),
            },
        ];

        render_project_diff::execute(
            RenderProjectDiff {
                root: crate::utils::repository_root("//fixture.invalid/repositories/scan-root"),
                repos: repos.clone(),
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &filters,
        )
        .unwrap();
        let first_html = store.artifact(&artifact_path).unwrap().html;
        filters
            .filters
            .save_extension_filter(
                &repository_root("//fixture.invalid/repositories/repo-a"),
                &ExtensionFilter::default(),
            )
            .unwrap();
        filters
            .filters
            .save_extension_filter(
                &repository_root("//fixture.invalid/repositories/repo-b"),
                &hiding_extensions(&["txt"]),
            )
            .unwrap();

        render_project_diff::execute(
            RenderProjectDiff {
                root: crate::utils::repository_root("//fixture.invalid/repositories/scan-root"),
                repos,
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &filters,
        )
        .unwrap();
        let second_html = store.artifact(&artifact_path).unwrap().html;

        assert!(first_html.contains(
            "repo-a:en-US:mirage:unified:compact:1|repo-b:en-US:mirage:unified:compact:2"
        ));
        assert!(second_html.contains(
            "repo-a:en-US:glacier:unified:compact:2|repo-b:en-US:glacier:unified:compact:1"
        ));
    }
}
