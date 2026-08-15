//! The `render_diff_all` vertical slice: render every already-filtered repo
//! (upstream present, unpushed count > 0 -- filtering stays cli-side) into one
//! tabbed artifact. Unlike `render_diff_subrepos`, this never skips an empty
//! view and propagates a build error instead of swallowing it as a skip --
//! matches `run_managed_all`'s current no-skip behavior exactly.

use std::path::PathBuf;

use gtl_models::diffs::DiffKind;
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget,
        batch::{RepoRef, dated_title, render_batch},
    },
    ports::{
        ArtifactMeta, ArtifactStore, Clock, GitClient, HtmlRenderer, UserSettingsLoadError,
        UserSettingsStore,
    },
    shared::notes::Note,
};

/// Render a tabbed diff artifact across every repo in `repos`, which the caller
/// already filtered to upstream-present and unpushed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiffAll {
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: PathBuf,
    pub repos: Vec<RepoRef>,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffAllOk {
    pub artifact: PathBuf,
    pub reused: bool,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering diff-all.
#[derive(Debug, thiserror::Error)]
pub enum RenderDiffAllError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Renders every requested repository through the diff ports.
#[cqrsy::command]
pub fn execute(
    req: RenderDiffAll,
    app_settings: &impl UserSettingsStore,
    source: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderDiffAllOk, RenderDiffAllError> {
    let RenderDiffAll { root, repos } = req;
    let store_root = super::artifacts::root(&root);
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

    let title = dated_title(clock, "diff-artifact all");
    let render_options = settings.viewer_render_options();
    let theme = settings.theme().map(|theme| theme.to_string());
    let html =
        renderer.build_tabbed_html(&title, &batch.views, render_options, theme.as_deref())?;
    let meta = ArtifactMeta {
        repo_root: root,
        repo_name: "all".to_string(),
        kind: DiffKind::WorkTree,
        commit_range: None,
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
        "diff-all: {} repo(s)",
        batch.views.len()
    )));
    notes.push(Note::info(format!("wrote {}", placed.path.display())));
    Ok(RenderDiffAllOk {
        artifact: placed.path,
        reused: placed.reused,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{
        diffs::{DiffExclusions, DiffKind},
        settings::UserSettings,
        viewer::{RenderOptions, Theme},
    };

    use super::{RenderDiffAll, RepoRef, execute};
    use crate::{
        shared::notes::Note,
        testing::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore, RepoOverride,
            SequenceUserSettingsStore, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn settings(theme: Theme, exclusions: DiffExclusions) -> UserSettings {
        UserSettings::new(Some(theme), RenderOptions::DEFAULT, true, exclusions)
    }

    fn req(repos: Vec<RepoRef>) -> RenderDiffAll {
        RenderDiffAll {
            root: PathBuf::from("/scan-root"),
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
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("render succeeds");

        assert_eq!(
            response.artifact,
            PathBuf::from("/scan-root/.artifacts/gtl/artifact.html")
        );
        assert!(!response.reused);
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-all: 2 repo(s)"),
                Note::info("wrote /scan-root/.artifacts/gtl/artifact.html"),
            ]
        );
        let artifact = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .expect("artifact persisted");
        assert_eq!(artifact.meta.repo_name, "all");
        assert_eq!(artifact.meta.kind, DiffKind::WorkTree);
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
            top: "/repo".into(),
            label: "repo".into(),
        }];

        let result = execute(
            req(repos),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
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
                DiffExclusions::new([("repo-a".to_string(), vec!["md"])], None),
            ),
            settings(
                Theme::Light,
                DiffExclusions::new([("repo-b".to_string(), vec!["txt"])], None),
            ),
        ]);
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

        execute(
            RenderDiffAll {
                root: PathBuf::from("/scan-root"),
                repos: repos.clone(),
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("first render succeeds");
        let first_html = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .expect("first artifact persisted")
            .html;

        execute(
            RenderDiffAll {
                root: PathBuf::from("/scan-root"),
                repos,
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock("2026-07-02T00:00:00Z".into()),
        )
        .expect("second render succeeds");
        let second_html = store
            .artifact(&PathBuf::from("/scan-root/.artifacts/gtl/artifact.html"))
            .expect("second artifact persisted")
            .html;

        assert!(
            first_html.contains("repo-a:hearth:unified:compact:1|repo-b:hearth:unified:compact:2")
        );
        assert!(
            second_html.contains("repo-a:light:unified:compact:2|repo-b:light:unified:compact:1")
        );
    }
}
