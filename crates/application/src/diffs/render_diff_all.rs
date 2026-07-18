//! The `render_diff_all` vertical slice: render every already-filtered repo
//! (upstream present, unpushed count > 0 -- filtering stays cli-side) into one
//! tabbed artifact. Unlike `render_diff_subrepos`, this never skips an empty
//! view and propagates a build error instead of swallowing it as a skip --
//! matches `run_managed_all`'s current no-skip behavior exactly.

use std::path::PathBuf;

use domain::diffs::DiffKind;
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget,
        batch::{RepoRef, dated_title, render_batch},
    },
    ports::{AppSettingsStore, ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render a tabbed diff preview across every repo in `repos` (already filtered
/// by the caller to upstream-present + unpushed > 0) under `store_root`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiffAll {
    pub store_root: PathBuf,
    /// Canonicalized scan root (used for `ArtifactMeta.repo_root`).
    pub root: PathBuf,
    pub repos: Vec<RepoRef>,
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
#[cqrsy::command]
pub fn execute(
    req: RenderDiffAll,
    app_settings: &impl AppSettingsStore,
    source: &impl DiffSource,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
) -> Result<RenderDiffAllResponse, RenderDiffAllError> {
    let RenderDiffAll {
        root,
        store_root,
        repos,
    } = req;
    let settings = app_settings.load();
    let mut notes = Vec::new();
    let batch = render_batch(
        source,
        &DiffTarget::Unpushed { pinned: None },
        &settings,
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
        theme: settings.theme().map(str::to_owned),
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
    use std::{
        collections::VecDeque,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use domain::diffs::{DiffExclusions, DiffKind};

    use super::{RenderDiffAll, RepoRef, execute};
    use crate::{
        ports::{AppSettings, AppSettingsStore},
        shared::notes::Note,
        testing::{
            FakeDiffSource, FixedAppSettingsStore, FixedClock, InMemoryArtifactStore, RepoOverride,
            StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    #[derive(Clone)]
    struct SequenceAppSettingsStore {
        snapshots: Arc<Mutex<VecDeque<AppSettings>>>,
    }

    impl SequenceAppSettingsStore {
        fn new(snapshots: impl IntoIterator<Item = AppSettings>) -> Self {
            Self {
                snapshots: Arc::new(Mutex::new(snapshots.into_iter().collect())),
            }
        }
    }

    impl AppSettingsStore for SequenceAppSettingsStore {
        fn load(&self) -> AppSettings {
            self.snapshots
                .lock()
                .expect("settings sequence lock")
                .pop_front()
                .expect("one settings snapshot per operation")
        }
    }

    fn req(repos: Vec<RepoRef>) -> RenderDiffAll {
        RenderDiffAll {
            store_root: PathBuf::from("/store"),
            root: PathBuf::from("/scan-root"),
            repos,
        }
    }

    #[test]
    fn renders_every_repo_without_skipping_empties() {
        let source = FakeDiffSource {
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
            &FixedAppSettingsStore::default(),
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
            &FixedAppSettingsStore::default(),
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
        let mut source = FakeDiffSource {
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
        let app_settings = SequenceAppSettingsStore::new([
            AppSettings::new(
                Some("first".into()),
                true,
                DiffExclusions::new([("repo-a".to_string(), vec!["md"])], None),
            ),
            AppSettings::new(
                Some("second".into()),
                true,
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
                store_root: PathBuf::from("/store"),
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
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("first artifact persisted")
            .html;

        execute(
            RenderDiffAll {
                store_root: PathBuf::from("/store"),
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
            .artifact(&PathBuf::from("/store/diffs/fake/artifact.html"))
            .expect("second artifact persisted")
            .html;

        assert!(first_html.contains("repo-a:first:1|repo-b:first:2"));
        assert!(second_html.contains("repo-a:second:2|repo-b:second:1"));
    }
}
