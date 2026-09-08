use std::path::PathBuf;

use gtl_models::{
    artifacts::{ArtifactCommitRange, ArtifactDiffIdentity, ArtifactRangeKind},
    diffs::{DiffKind, PinnedRange},
    git::{GitDiffSpec, GitRevision},
    paths::RepositoryRoot,
};
use serde::{Deserialize, Serialize};

use crate::{
    diffs::{
        DiffTarget, DiffTargetRequest, DiffTargetRequestError, diff_computation, range::DiffRanges,
    },
    ports::{
        ArtifactMeta, ArtifactRangeKey, ArtifactStore, Clock, GitClient, HtmlRenderer,
        PlacedArtifact, UserSettingsLoadError, UserSettingsReader,
    },
    shared::notes::Note,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDiff {
    pub cwd: PathBuf,
    pub target: DiffTargetRequest,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderDiffOk {
    pub rendered_repositories: Vec<gtl_models::paths::RepositoryRoot>,
    pub outcome: RenderDiffOutcome,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderDiffOutcome {
    Rendered(PlacedArtifact),
    Empty,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderDiffError {
    #[error(transparent)]
    Comparison(#[from] crate::projects::comparison::ComparisonError),
    #[error(transparent)]
    InvalidTarget(#[from] DiffTargetRequestError),
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

fn resolved_range(
    git: &impl GitClient,
    top: &RepositoryRoot,
    target: &DiffTarget,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Option<ArtifactCommitRange> {
    let (kind, range) = match target {
        DiffTarget::Range {
            pinned: Some(range),
            ..
        }
        | DiffTarget::Last {
            pinned: Some(range),
            ..
        }
        | DiffTarget::Unpushed {
            pinned: Some(range),
        } => {
            return Some(ArtifactCommitRange {
                kind: ArtifactRangeKind::TwoDot,
                commits: range.clone(),
            });
        }
        DiffTarget::Merge {
            pinned: Some(range),
            ..
        } => {
            return Some(ArtifactCommitRange {
                kind: ArtifactRangeKind::ThreeDot,
                commits: range.clone(),
            });
        }
        DiffTarget::Range {
            range,
            pinned: None,
        } => {
            let diff_range = DiffRanges::exact(range.clone()).diff;
            (DiffKind::TwoDot, diff_range)
        }
        DiffTarget::Last {
            count,
            pinned: None,
        } => (
            DiffKind::TwoDot,
            DiffRanges::exact(gtl_models::git::GitRange::head_commits(*count)).diff,
        ),
        DiffTarget::Merge { base, pinned: None } => {
            (DiffKind::ThreeDot, DiffRanges::merge(base).diff)
        }
        DiffTarget::Unpushed { pinned: None } => {
            let comparison = crate::projects::comparison::resolve(top, git, comparisons).ok()?;
            let kind = match comparison {
                crate::projects::comparison::ResolvedComparison::Upstream { .. } => {
                    ArtifactRangeKind::TwoDot
                }
                crate::projects::comparison::ResolvedComparison::Branch { .. } => {
                    ArtifactRangeKind::ThreeDot
                }
            };
            return Some(ArtifactCommitRange {
                kind,
                commits: comparison.pin(top, git).ok()?,
            });
        }

        DiffTarget::Base(_) => return None,
    };
    let base = git.resolve_commit_id(top, &range_base(&range)?).ok()?;
    let head = git.resolve_commit_id(top, &range_head(&range)?).ok()?;
    Some(ArtifactCommitRange {
        kind: ArtifactRangeKind::try_from(kind).ok()?,
        commits: PinnedRange { base, head },
    })
}

#[cqrsy::command]
pub fn execute(
    req: RenderDiff,
    app_settings: &impl UserSettingsReader,
    git: &impl GitClient,
    store: &impl ArtifactStore,
    renderer: &impl HtmlRenderer,
    clock: &impl Clock,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<RenderDiffOk, RenderDiffError> {
    let RenderDiff { cwd, target, name } = req;
    let target = DiffTarget::try_from(target)?;
    let mut notes = Vec::new();
    let settings = app_settings.load()?;
    let render_options = settings.viewer_render_options();
    let theme = settings.theme();
    let top = git.top_level(&cwd)?;
    let store_root = super::artifacts::root(top.as_ref());
    let excluded = settings
        .diff_exclusions()
        .for_project_or_default(&top.project_name());

    if name.is_none()
        && let Some(range) = resolved_range(git, &top, &target, comparisons)
        && let Some(hit) = store.lookup_by_range(
            &store_root,
            &top,
            &ArtifactRangeKey {
                range,
                render_options,
                theme,
                excluded_extensions: excluded.clone(),
            },
        )?
    {
        notes.push(Note::info(format!(
            "diff-artifact: reusing {}",
            hit.display()
        )));
        return Ok(RenderDiffOk {
            rendered_repositories: vec![top.clone()],
            outcome: RenderDiffOutcome::Rendered(PlacedArtifact::Reused { path: hit }),
            notes,
        });
    }

    let commit_range = resolved_range(git, &top, &target, comparisons).map(|range| range.commits);
    let computed =
        diff_computation::build(git, &top, &target, settings.diff_exclusions(), comparisons)?;
    let mut view = computed.view;
    let summary = computed.summary;
    notes.extend(computed.notes);
    if let Some(name) = &name {
        view.title.clone_from(name);
    }
    if !view.has_diff_content() {
        notes.push(Note::warn(format!(
            "diff-artifact: {summary}, nothing to show (no commits or changes); skipping"
        )));
        return Ok(RenderDiffOk {
            rendered_repositories: vec![top.clone()],
            outcome: RenderDiffOutcome::Empty,
            notes,
        });
    }
    let file_count = view.files.len();
    let html = renderer.build_html(&view, render_options, theme)?;

    let meta = ArtifactMeta {
        repo_root: top.clone(),
        repo_name: view.repo_name.clone(),
        identity: ArtifactDiffIdentity::from_parts(
            DiffKind::from_diff_range(&view.cmd.range),
            commit_range,
        )
        .map_err(anyhow::Error::from)?,
        range_label: view.cmd.range.clone(),
        head_committed_at: git.committed_at(&top, &GitRevision::head()),
        generated_at: clock.now().map_err(anyhow::Error::from)?,
        title: view.title.clone(),
        render_options,
        theme,
        excluded_extensions: excluded.clone(),
    };
    let placed = store.place(&store_root, &meta, &html)?;

    notes.push(Note::info(format!(
        "diff-artifact: {summary}, {}",
        legacy_count_label(file_count, "file")
    )));
    notes.push(Note::info(format!("wrote {}", placed.path().display())));
    Ok(RenderDiffOk {
        rendered_repositories: vec![top.clone()],
        outcome: RenderDiffOutcome::Rendered(placed),
        notes,
    })
}

fn range_base(spec: &GitDiffSpec) -> Option<GitRevision> {
    let base = spec
        .as_arg()
        .split("..")
        .next()
        .unwrap_or(spec.as_arg())
        .trim_end_matches('.');
    GitRevision::try_new(base.to_owned()).ok()
}

fn range_head(spec: &GitDiffSpec) -> Option<GitRevision> {
    let head = spec.as_arg().rsplit("..").next()?;
    GitRevision::try_new(head.to_owned()).ok()
}

fn legacy_count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}(s)")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::{
        artifacts::{ArtifactCommitRange, ArtifactRangeKind},
        diffs::{DiffExclusions, ExcludedExtensions},
        settings::UserSettings,
        viewer::{RenderOptions, Theme},
    };

    use super::{RenderDiff, RenderDiffError, RenderDiffOutcome};
    use crate::{
        diffs::{DiffTarget, DiffTargetRequest, render_diff},
        ports::ArtifactRangeKey,
        shared::notes::Note,
        utils::{
            FakeGitClient, FixedClock, FixedUserSettingsStore, InMemoryArtifactStore, StubRenderer,
            diffs::{DIFF_SINGLE_FILE, commit},
        },
    };

    fn req(source_top: &str, target: &DiffTarget) -> RenderDiff {
        RenderDiff {
            cwd: PathBuf::from(source_top),
            target: DiffTargetRequest::from(target),
            name: None,
        }
    }

    fn settings(theme: Option<Theme>, exclusions: DiffExclusions) -> UserSettings {
        UserSettings::new(
            theme,
            RenderOptions::DEFAULT,
            gtl_models::viewer::ViewerKeybindings::default(),
            true,
            exclusions,
            gtl_models::settings::PushAllExclusions::default(),
        )
    }

    fn range_key(base_id: &str, head_id: &str) -> ArtifactRangeKey {
        ArtifactRangeKey {
            range: ArtifactCommitRange {
                kind: ArtifactRangeKind::TwoDot,
                commits: crate::utils::pinned_range(base_id, head_id),
            },
            render_options: RenderOptions::DEFAULT,
            theme: None,
            excluded_extensions: ExcludedExtensions::default(),
        }
    }

    #[test]
    fn renders_and_stores_an_artifact_with_the_summary_notes() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            committed_at: Some(
                gtl_models::timestamps::MachineTimestamp::try_from("2026-07-02T00:00:00Z").unwrap(),
            ),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = render_diff::execute(
            req("/repo", &DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(
            response.outcome,
            RenderDiffOutcome::Rendered(crate::ports::PlacedArtifact::Created {
                path: crate::utils::absolute_file_path("/repo/.artifacts/gtl/artifact.html"),
            })
        );
        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert_eq!(artifact.meta.title, "diff");
        assert_eq!(artifact.meta.repo_name, crate::utils::project_name("repo"));
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-artifact: 1 unpushed commit(s), 1 file(s)"),
                Note::info("wrote /repo/.artifacts/gtl/artifact.html"),
            ]
        );
    }

    #[test]
    fn render_uses_the_resolved_projects_settings_snapshot() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        let app_settings = FixedUserSettingsStore::new(settings(
            Some(Theme::Noir),
            DiffExclusions::new(
                [(crate::utils::project_name("repo"), vec!["md".to_string()])],
                Some(vec!["txt".to_string()]),
            ),
        ));

        render_diff::execute(
            RenderDiff {
                cwd: PathBuf::from("/repo"),
                target: DiffTargetRequest::Unpushed,
                name: None,
            },
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert_eq!(artifact.meta.excluded_extensions.extensions(), ["md"]);
        assert!(artifact.html.contains("noir"));
    }

    #[test]
    fn empty_view_returns_empty_with_the_skip_warning() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![],
            diff_output: String::new(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = render_diff::execute(
            req("/repo", &DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(response.outcome, RenderDiffOutcome::Empty);
        assert_eq!(
            response.notes,
            vec![Note::warn(
                "diff-artifact: 0 unpushed commit(s), nothing to show (no commits or changes); skipping"
            )]
        );
    }

    #[test]
    fn pure_range_reuses_an_existing_artifact() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commit_ids: [
                ("a".to_string(), crate::utils::commit_id_fixture("id-a")),
                ("b".to_string(), crate::utils::commit_id_fixture("id-b")),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(range_key("id-a", "id-b"), "/store/existing.html");

        let response = render_diff::execute(
            req(
                "/repo",
                &DiffTarget::Range {
                    range: crate::utils::git_range("a..b"),
                    pinned: None,
                },
            ),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(
            response.outcome,
            RenderDiffOutcome::Rendered(crate::ports::PlacedArtifact::Reused {
                path: crate::utils::absolute_file_path("/store/existing.html"),
            })
        );
        assert_eq!(
            response.notes,
            vec![Note::info("diff-artifact: reusing /store/existing.html")]
        );
    }

    #[test]
    fn fast_path_never_reuses_an_artifact_rendered_under_a_different_exclusion_set() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            commit_ids: [
                ("a".to_string(), crate::utils::commit_id_fixture("id-a")),
                ("b".to_string(), crate::utils::commit_id_fixture("id-b")),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(range_key("id-a", "id-b"), "/store/unfiltered.html");

        let request = req(
            "/repo",
            &DiffTarget::Range {
                range: crate::utils::git_range("a..b"),
                pinned: None,
            },
        );
        let app_settings = FixedUserSettingsStore::new(settings(
            None,
            DiffExclusions::new(
                [(crate::utils::project_name("repo"), vec!["md".to_string()])],
                None,
            ),
        ));
        let response = render_diff::execute(
            request,
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered(crate::ports::PlacedArtifact::Created { .. })
        ));
        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert_eq!(
            artifact.meta.excluded_extensions.extensions(),
            ["md"],
            "the active set must be recorded for future range lookups"
        );
    }

    #[test]
    fn fast_path_never_reuses_an_artifact_rendered_under_a_different_theme() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            commit_ids: [
                ("a".to_string(), crate::utils::commit_id_fixture("id-a")),
                ("b".to_string(), crate::utils::commit_id_fixture("id-b")),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(
            ArtifactRangeKey {
                theme: Some(Theme::Dark),
                ..range_key("id-a", "id-b")
            },
            "/store/dark.html",
        );

        let response = render_diff::execute(
            req(
                "/repo",
                &DiffTarget::Range {
                    range: crate::utils::git_range("a..b"),
                    pinned: None,
                },
            ),
            &FixedUserSettingsStore::new(settings(Some(Theme::Light), DiffExclusions::default())),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered(crate::ports::PlacedArtifact::Created { .. })
        ));
    }

    #[test]
    fn fast_path_reuses_an_artifact_rendered_under_the_same_exclusion_set() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commit_ids: [
                ("a".to_string(), crate::utils::commit_id_fixture("id-a")),
                ("b".to_string(), crate::utils::commit_id_fixture("id-b")),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(
            ArtifactRangeKey {
                excluded_extensions: ExcludedExtensions::new(["md"]),
                ..range_key("id-a", "id-b")
            },
            "/store/filtered.html",
        );

        let request = req(
            "/repo",
            &DiffTarget::Range {
                range: crate::utils::git_range("a..b"),
                pinned: None,
            },
        );
        let app_settings = FixedUserSettingsStore::new(settings(
            None,
            DiffExclusions::new(
                [(crate::utils::project_name("repo"), vec!["md".to_string()])],
                None,
            ),
        ));
        let response = render_diff::execute(
            request,
            &app_settings,
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert_eq!(
            response.outcome,
            RenderDiffOutcome::Rendered(crate::ports::PlacedArtifact::Reused {
                path: crate::utils::absolute_file_path("/store/filtered.html"),
            })
        );
    }

    #[test]
    fn named_run_skips_the_fast_path_and_overrides_the_title() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            commit_ids: [
                ("a".to_string(), crate::utils::commit_id_fixture("id-a")),
                ("b".to_string(), crate::utils::commit_id_fixture("id-b")),
            ]
            .into_iter()
            .collect(),
            known_revs: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();
        store.range_hit_insert(range_key("id-a", "id-b"), "/store/existing.html");

        let mut request = req(
            "/repo",
            &DiffTarget::Range {
                range: crate::utils::git_range("a..b"),
                pinned: None,
            },
        );
        request.name = Some("custom".into());
        let response = render_diff::execute(
            request,
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert!(matches!(
            response.outcome,
            RenderDiffOutcome::Rendered(crate::ports::PlacedArtifact::Created { .. })
        ));
        let artifact = store
            .artifact(&PathBuf::from("/repo/.artifacts/gtl/artifact.html"))
            .unwrap();
        assert_eq!(artifact.meta.title, "custom");
    }

    #[test]
    fn unpushed_without_upstream_renders_committed_branch_changes() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            known_revs: vec!["refs/heads/main".into(), "HEAD".into()],
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let response = render_diff::execute(
            req("/repo", &DiffTarget::Unpushed { pinned: None }),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert!(matches!(response.outcome, RenderDiffOutcome::Rendered(_)));
        assert_eq!(
            response.notes,
            vec![
                Note::info("diff-artifact: no upstream; comparing branch changes against main"),
                Note::info("diff-artifact: branch changes against refs/heads/main, 1 file(s)"),
                Note::info("wrote /repo/.artifacts/gtl/artifact.html"),
            ]
        );
    }

    #[test]
    fn unknown_base_is_an_error() {
        let source = FakeGitClient {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            known_revs: vec![],
            ..Default::default()
        };
        let store = InMemoryArtifactStore::default();

        let error = render_diff::execute(
            req(
                "/repo",
                &DiffTarget::Base(crate::utils::git_revision("nope")),
            ),
            &FixedUserSettingsStore::default(),
            &source,
            &store,
            &StubRenderer,
            &FixedClock::from_raw("2026-07-02T00:00:00Z"),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap_err();

        let err = match error {
            RenderDiffError::Unexpected(error)
            | RenderDiffError::Comparison(
                crate::projects::comparison::ComparisonError::Unexpected(error),
            ) => Some(error),
            RenderDiffError::InvalidTarget(_)
            | RenderDiffError::Settings(_)
            | RenderDiffError::Comparison(_) => None,
        }
        .unwrap();
        assert_eq!(format!("{err:#}"), "unknown revision nope");
    }
}
