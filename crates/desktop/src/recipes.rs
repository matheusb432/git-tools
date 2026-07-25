//! Recipe compute orchestration through direct application operations.

use std::sync::{Arc, Mutex};

use application::{
    diffs::{
        DiffTarget, PinnedRange, View,
        compute_diff::{self, ComputeDiff},
        compute_merge_diff::{self, ComputeMergeDiff},
        compute_squash_preview::{self, ComputeSquashPreview},
    },
    history::record_render::{self, RecordRender},
    live_views::probe::{self, ProbeOutcome, ProbeSource},
    ports::{AppStateStore, Clock, DiffSource, RepoProbe, UserSettingsStore},
    viewer::{ViewerTabId, ViewerTabKind, ViewerTabState},
};
use gtl_recipe::{Recipe, RecipeOp, RecipeSource, RecipeTarget};
use infra::user_config::TomlSettingsStore;

use crate::{
    session::{CachedView, ComputeTicket, PublishOutcome, ViewerSession},
    tab_label,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecipeError {
    Failed(String),
    Stale,
}

impl std::fmt::Display for RecipeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(reason) => formatter.write_str(reason),
            Self::Stale => formatter.write_str("recipe computation was superseded"),
        }
    }
}

impl std::error::Error for RecipeError {}

impl From<String> for RecipeError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}

/// The reserved tab and a current computed view, when the tab is renderable.
#[derive(Debug, Clone)]
pub(crate) struct OpenedRecipe {
    pub(crate) tab_id: ViewerTabId,
    pub(crate) ticket: ComputeTicket,
    pub(crate) view: Option<Arc<View>>,
}

#[derive(Debug, Clone)]
pub(crate) enum OpenRecipeOutcome {
    Opened(OpenedRecipe),
    Skipped { label: String },
}

#[derive(Debug, Clone)]
pub(crate) struct RefreshedRecipe {
    pub(crate) ticket: ComputeTicket,
    pub(crate) view: Option<Arc<View>>,
}

#[derive(Debug, Clone)]
enum ComputationOutcome {
    Rendered(Arc<View>),
    Skipped { label: String },
    StateOnly,
}

#[derive(Debug)]
pub(crate) struct RecipeContext<'a, Source, Probe, State, Time> {
    source: &'a Source,
    probe: &'a Probe,
    app_state: &'a State,
    clock: &'a Time,
    user_settings: &'a TomlSettingsStore,
}

impl<Source, Probe, State, Time> Copy for RecipeContext<'_, Source, Probe, State, Time> {}

impl<Source, Probe, State, Time> Clone for RecipeContext<'_, Source, Probe, State, Time> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, Source, Probe, State, Time> RecipeContext<'a, Source, Probe, State, Time> {
    pub(crate) fn new(
        source: &'a Source,
        probe: &'a Probe,
        app_state: &'a State,
        clock: &'a Time,
        user_settings: &'a TomlSettingsStore,
    ) -> Self {
        Self {
            source,
            probe,
            app_state,
            clock,
            user_settings,
        }
    }
}

impl ComputationOutcome {
    fn into_view(self) -> Option<Arc<View>> {
        match self {
            Self::Rendered(view) => Some(view),
            Self::Skipped { .. } | Self::StateOnly => None,
        }
    }
}

/// Carry an optional pin across the crate boundary: `gtl_recipe::PinnedRange`
/// to `application::diffs::PinnedRange`.
fn to_application_pin(pinned: Option<&gtl_recipe::PinnedRange>) -> Option<PinnedRange> {
    pinned.map(|pin| PinnedRange {
        base: pin.base.clone(),
        head: pin.head.clone(),
    })
}

fn diff_target(target: &RecipeTarget) -> DiffTarget {
    match target {
        RecipeTarget::Unpushed { pinned } => DiffTarget::Unpushed {
            pinned: to_application_pin(pinned.as_ref()),
        },
        RecipeTarget::Base { rev } => DiffTarget::Base(rev.clone()),
        RecipeTarget::Range { range, pinned } => DiffTarget::Range {
            range: range.clone(),
            pinned: to_application_pin(pinned.as_ref()),
        },
        RecipeTarget::Merge { base, pinned } => DiffTarget::Merge {
            base: base.clone(),
            pinned: to_application_pin(pinned.as_ref()),
        },
        RecipeTarget::Last { count, pinned } => DiffTarget::Last {
            count: *count,
            pinned: to_application_pin(pinned.as_ref()),
        },
    }
}

pub(crate) fn compute_view(
    source: &impl DiffSource,
    recipe: &Recipe,
    user_settings: &impl UserSettingsStore,
) -> Result<View, String> {
    let cwd = recipe.cwd();
    match &recipe.op {
        RecipeOp::Diff { target } => compute_diff::execute(
            ComputeDiff {
                cwd,
                target: diff_target(target),
            },
            user_settings,
            source,
        )
        .map(|response| response.view)
        .map_err(|error| format!("{error:#}")),
        RecipeOp::MergeDiff { base, pinned } => compute_merge_diff::execute(
            ComputeMergeDiff {
                cwd,
                base: base.clone(),
                pinned: to_application_pin(pinned.as_ref()),
            },
            user_settings,
            source,
        )
        .map(|response| response.view)
        .map_err(|error| format!("{error:#}")),
        RecipeOp::SquashPreview { pinned } => compute_squash_preview::execute(
            ComputeSquashPreview {
                cwd,
                pinned: to_application_pin(pinned.as_ref()),
            },
            user_settings,
            source,
        )
        .map(|response| response.view)
        .map_err(|error| format!("{error:#}")),
    }
}

pub(crate) fn open_recipe(
    context: RecipeContext<'_, impl DiffSource, impl RepoProbe, impl AppStateStore, impl Clock>,
    session: &Mutex<ViewerSession>,
    recipe: &Recipe,
    batch_id: String,
    kind: ViewerTabKind,
) -> Result<OpenRecipeOutcome, RecipeError> {
    let (id, ticket) = {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        let id = session.open(recipe.clone(), batch_id, kind);
        let ticket = session
            .begin_compute(id)
            .ok_or_else(|| format!("tab {id} closed before compute"))?;
        (id, ticket)
    };

    match compute_and_publish(&context, session, recipe, kind, ticket)? {
        ComputationOutcome::Rendered(view) => Ok(OpenRecipeOutcome::Opened(OpenedRecipe {
            tab_id: id,
            ticket,
            view: Some(view),
        })),
        ComputationOutcome::StateOnly => Ok(OpenRecipeOutcome::Opened(OpenedRecipe {
            tab_id: id,
            ticket,
            view: None,
        })),
        ComputationOutcome::Skipped { label } => Ok(OpenRecipeOutcome::Skipped { label }),
    }
}

#[cfg(test)]
pub(crate) fn refresh_recipe(
    context: RecipeContext<'_, impl DiffSource, impl RepoProbe, impl AppStateStore, impl Clock>,
    session: &Mutex<ViewerSession>,
    id: ViewerTabId,
) -> Result<Option<Arc<View>>, String> {
    let (recipe, kind, ticket) = {
        let mut session = session.lock().map_err(|error| error.to_string())?;
        let tab = session.tab(id).ok_or_else(|| format!("unknown tab {id}"))?;
        let recipe = tab.recipe.clone();
        let kind = tab.tab.kind();
        let ticket = session
            .refresh(id)
            .ok_or_else(|| format!("unknown tab {id}"))?;
        (recipe, kind, ticket)
    };

    compute_and_publish(&context, session, &recipe, kind, ticket)
        .map(ComputationOutcome::into_view)
        .map_err(|error| error.to_string())
}

pub(crate) fn refresh_recipe_versioned(
    context: RecipeContext<'_, impl DiffSource, impl RepoProbe, impl AppStateStore, impl Clock>,
    session: &Mutex<ViewerSession>,
    id: ViewerTabId,
) -> Result<RefreshedRecipe, RecipeError> {
    let (recipe, kind, ticket) = {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        let tab = session.tab(id).ok_or_else(|| format!("unknown tab {id}"))?;
        let recipe = tab.recipe.clone();
        let kind = tab.tab.kind();
        let ticket = session
            .refresh(id)
            .ok_or_else(|| format!("unknown tab {id}"))?;
        (recipe, kind, ticket)
    };
    let view = compute_and_publish(&context, session, &recipe, kind, ticket)?.into_view();
    Ok(RefreshedRecipe { ticket, view })
}

fn compute_and_publish(
    context: &RecipeContext<'_, impl DiffSource, impl RepoProbe, impl AppStateStore, impl Clock>,
    session: &Mutex<ViewerSession>,
    recipe: &Recipe,
    kind: ViewerTabKind,
    ticket: ComputeTicket,
) -> Result<ComputationOutcome, RecipeError> {
    if kind == ViewerTabKind::Live
        && let Some(broken) = probe_live_source(context.probe, recipe)?
    {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        if session.set_state_if_current(ticket, broken) == PublishOutcome::Stale {
            return Err(RecipeError::Stale);
        }
        return Ok(ComputationOutcome::StateOnly);
    }

    let view = match compute_view(context.source, recipe, context.user_settings) {
        Ok(view) => Arc::new(view),
        Err(reason) => {
            publish_compute_error(session, ticket, &reason)?;
            return Ok(ComputationOutcome::StateOnly);
        }
    };
    if kind == ViewerTabKind::Snapshot && !view.has_diff_content() {
        let label = tab_label::initial(recipe);
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        return match session.close_if_current(ticket) {
            PublishOutcome::Published => Ok(ComputationOutcome::Skipped { label }),
            PublishOutcome::Stale => Err(RecipeError::Stale),
        };
    }
    let published = {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        session.publish_if_current(ticket, CachedView::new(Arc::clone(&view)))
    };
    if published == PublishOutcome::Published {
        record_render(context.app_state, context.clock, recipe, &view);
        return Ok(ComputationOutcome::Rendered(view));
    }
    Err(RecipeError::Stale)
}

fn publish_compute_error(
    session: &Mutex<ViewerSession>,
    ticket: ComputeTicket,
    reason: &str,
) -> Result<(), RecipeError> {
    eprintln!("gtl-viewer compute failed: {reason}");
    let mut session = session
        .lock()
        .map_err(|error| RecipeError::Failed(error.to_string()))?;
    match session.set_state_if_current(
        ticket,
        ViewerTabState::Error {
            reason: "The diff could not be rendered. Please retry.".into(),
        },
    ) {
        PublishOutcome::Published => Ok(()),
        PublishOutcome::Stale => Err(RecipeError::Stale),
    }
}

fn probe_live_source(
    probe: &impl RepoProbe,
    recipe: &Recipe,
) -> Result<Option<ViewerTabState>, String> {
    let RecipeSource::LocalRepo(path) = &recipe.source;
    let response = probe::execute(
        ProbeSource {
            source_kind: "LocalRepo".into(),
            source_value: path.display().to_string(),
        },
        probe,
    )
    .map_err(|error| format!("{error:#}"))?;
    Ok(match response.outcome {
        ProbeOutcome::Ok => None,
        ProbeOutcome::Broken { rejection } => Some(ViewerTabState::Broken {
            code: rejection.code().into(),
            reason: rejection.to_string(),
        }),
    })
}

fn record_render(app_state: &impl AppStateStore, clock: &impl Clock, recipe: &Recipe, view: &View) {
    if let Err(error) = record_render::execute(
        RecordRender {
            recipe: recipe.clone(),
            title: tab_label::computed(recipe, view),
            repo_name: view.repo_name.clone(),
            range_label: view.cmd.range.clone(),
        },
        app_state,
        clock,
    ) {
        eprintln!("gtl-viewer: failed to record render history: {error:#}");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use application::{
        history::{RecentRenderRecord, list_recent_renders},
        ports::{AppStateStore, RepoProbeResult},
        testing::{FakeDiffSource, FakeRepoProbe, FixedClock},
    };
    use domain::{
        diffs::Commit,
        viewer::{ViewerTabKind, ViewerTabState},
    };
    use gtl_recipe::{Recipe, RecipeOp, RecipeSource, RecipeTarget};
    use infra::app_state::SqliteAppState;
    use tempfile::TempDir;

    use super::*;
    use crate::session::ViewerSession;

    const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1 @@\n\
-old\n\
+new\n";

    struct RecipeFixture {
        _app_state_directory: TempDir,
        app_state: SqliteAppState,
    }

    impl RecipeFixture {
        fn new() -> Self {
            let app_state_directory = tempfile::tempdir().expect("temporary app-state directory");
            let app_state =
                SqliteAppState::open(app_state_directory.path()).expect("open app state");
            Self {
                _app_state_directory: app_state_directory,
                app_state,
            }
        }

        fn history(&self) -> Vec<RecentRenderRecord> {
            list_recent_renders::execute(list_recent_renders::ListRecentRenders, &self.app_state)
                .expect("list render history")
                .entries
        }

        fn history_clear(&self) {
            self.app_state
                .connection_lock()
                .expect("connection lock")
                .execute("DELETE FROM recent_renders", [])
                .expect("clear render history");
        }
    }

    fn recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo("/repo".into()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn source() -> FakeDiffSource {
        FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![Commit {
                sha: "abc1234".into(),
                subject: "feat: work".into(),
                ..Default::default()
            }],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        }
    }

    fn empty_source() -> FakeDiffSource {
        FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            ..Default::default()
        }
    }

    fn expect_opened(outcome: OpenRecipeOutcome) -> OpenedRecipe {
        match outcome {
            OpenRecipeOutcome::Opened(opened) => opened,
            OpenRecipeOutcome::Skipped { label } => {
                panic!("expected an opened recipe, skipped {label}")
            }
        }
    }

    #[test]
    fn empty_snapshot_is_skipped_without_history() {
        let fixture = RecipeFixture::new();
        let source = empty_source();
        let probe = FakeRepoProbe {
            result: RepoProbeResult::Repo {
                top_level: "/repo".into(),
            },
        };
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(1024));

        let outcome = open_recipe(
            RecipeContext::new(
                &source,
                &probe,
                &fixture.app_state,
                &clock,
                &TomlSettingsStore::new(None),
            ),
            &session,
            &recipe(),
            "snapshot".into(),
            ViewerTabKind::Snapshot,
        )
        .expect("empty snapshot is a successful skip");

        let OpenRecipeOutcome::Skipped { label } = outcome else {
            panic!("empty snapshot must be skipped");
        };
        assert_eq!(label, "repo: diff");
        assert!(session.lock().expect("session").tabs().next().is_none());
        assert!(fixture.history().is_empty());
    }

    #[test]
    fn empty_live_view_remains_ready_and_refreshable() {
        let fixture = RecipeFixture::new();
        let source = empty_source();
        let probe = FakeRepoProbe {
            result: RepoProbeResult::Repo {
                top_level: "/repo".into(),
            },
        };
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(1024));

        let outcome = open_recipe(
            RecipeContext::new(
                &source,
                &probe,
                &fixture.app_state,
                &clock,
                &TomlSettingsStore::new(None),
            ),
            &session,
            &recipe(),
            "live".into(),
            ViewerTabKind::Live,
        )
        .expect("empty live view opens");

        let OpenRecipeOutcome::Opened(opened) = outcome else {
            panic!("empty live view must remain open");
        };
        assert!(
            opened
                .view
                .as_ref()
                .is_some_and(|view| !view.has_diff_content())
        );
        let session = session.lock().expect("session");
        assert_eq!(
            session.tab(opened.tab_id).expect("tab").tab.state(),
            &ViewerTabState::Ready
        );
    }

    #[test]
    fn open_computes_publishes_and_records_only_the_current_view() {
        let fixture = RecipeFixture::new();
        let source = source();
        let probe = FakeRepoProbe {
            result: RepoProbeResult::Repo {
                top_level: "/repo".into(),
            },
        };
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));

        let opened = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &recipe(),
                "batch-1".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("open succeeds"),
        );
        let id = opened.tab_id;
        assert!(opened.view.is_some());

        let mut session = session.lock().expect("session lock");
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Ready
        );
        assert_eq!(session.cached_view(id).expect("view").view.files.len(), 1);
        drop(session);
        let renders = fixture.history();
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].recipe.kind_tag(), "diff");
        assert_eq!(renders[0].title, "repo: 1 commit");
    }

    #[test]
    fn failed_initial_compute_preserves_the_explicit_recipe_label() {
        let source = FakeDiffSource::default();
        let probe = FakeRepoProbe::default();
        let fixture = RecipeFixture::new();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(1024));
        let mut named = recipe();
        named.name = Some("Named initial failure".into());

        let opened = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &named,
                "batch".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("published compute errors are acknowledged"),
        );
        assert!(opened.view.is_none());

        let session = session.lock().expect("session lock");
        let tab = session.tabs().next().expect("failed tab remains open");
        assert_eq!(tab.tab.label(), "Named initial failure");
        assert!(matches!(tab.tab.state(), ViewerTabState::Error { .. }));
    }

    #[test]
    fn oversize_open_returns_the_view_without_retaining_it() {
        let source = source();
        let probe = FakeRepoProbe::default();
        let fixture = RecipeFixture::new();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(1));

        let opened = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &recipe(),
                "batch".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("open succeeds"),
        );

        assert!(opened.view.is_some());
        assert!(
            session
                .lock()
                .expect("session lock")
                .cached_view(opened.tab_id)
                .is_none()
        );
    }

    #[test]
    fn broken_live_source_is_published_without_invoking_diff_compute() {
        let fixture = RecipeFixture::new();
        let source = FakeDiffSource::default();
        let probe = FakeRepoProbe {
            result: RepoProbeResult::NotFound,
        };
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(1024));

        let id = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &recipe(),
                "live".into(),
                ViewerTabKind::Live,
            )
            .expect("broken source remains an open tab"),
        )
        .tab_id;

        let session = session.lock().expect("session lock");
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Broken {
                code: "DirNotFound".into(),
                reason: "The git repo's directory at `/repo` was not found.".into(),
            }
        );
        assert!(fixture.history().is_empty());
    }

    #[test]
    fn non_repo_live_source_is_published_without_invoking_diff_compute() {
        let fixture = RecipeFixture::new();
        let source = FakeDiffSource::default();
        let probe = FakeRepoProbe {
            result: RepoProbeResult::NotAGitRepo,
        };
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(1024));

        let id = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &recipe(),
                "live".into(),
                ViewerTabKind::Live,
            )
            .expect("broken source remains an open tab"),
        )
        .tab_id;

        let session = session.lock().expect("session lock");
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Broken {
                code: "DirNotGitRepo".into(),
                reason: "The directory `/repo` is not a git repository.".into(),
            }
        );
        assert!(fixture.history().is_empty());
    }

    #[test]
    fn refresh_after_snapshot_to_live_reopen_probes_before_compute() {
        let fixture = RecipeFixture::new();
        let recipe = recipe();
        let source = source();
        let probe = FakeRepoProbe {
            result: RepoProbeResult::NotFound,
        };
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));
        let id = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &recipe,
                "snapshot".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("snapshot computes without probing"),
        )
        .tab_id;
        {
            let mut session = session.lock().expect("session lock");
            assert_eq!(session.open(recipe, "live".into(), ViewerTabKind::Live), id);
        }
        fixture.history_clear();

        let refreshed = refresh_recipe(
            RecipeContext::new(
                &source,
                &probe,
                &fixture.app_state,
                &clock,
                &TomlSettingsStore::new(None),
            ),
            &session,
            id,
        )
        .expect("broken live refresh remains a tab");

        assert!(refreshed.is_none());
        let session = session.lock().expect("session lock");
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Broken {
                code: "DirNotFound".into(),
                reason: "The git repo's directory at `/repo` was not found.".into(),
            }
        );
        assert!(fixture.history().is_empty());
    }

    #[test]
    fn refresh_recomputes_the_reserved_tab_and_records_the_current_result() {
        let fixture = RecipeFixture::new();
        let source = source();
        let probe = FakeRepoProbe::default();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));
        let id = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &recipe(),
                "batch".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("open succeeds"),
        )
        .tab_id;
        fixture.history_clear();

        refresh_recipe(
            RecipeContext::new(
                &source,
                &probe,
                &fixture.app_state,
                &clock,
                &TomlSettingsStore::new(None),
            ),
            &session,
            id,
        )
        .expect("refresh succeeds");

        assert_eq!(fixture.history().len(), 1);
        let mut session = session.lock().expect("session lock");
        assert!(session.cached_view(id).is_some());
    }

    #[test]
    fn failed_refresh_preserves_the_explicit_recipe_label() {
        let fixture = RecipeFixture::new();
        let successful_source = source();
        let failing_source = FakeDiffSource::default();
        let probe = FakeRepoProbe::default();
        let clock = FixedClock("2026-07-07T00:00:00Z".into());
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));
        let mut named = recipe();
        named.name = Some("Named refresh failure".into());
        let id = expect_opened(
            open_recipe(
                RecipeContext::new(
                    &successful_source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                &named,
                "batch".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("initial open succeeds"),
        )
        .tab_id;

        assert!(
            refresh_recipe(
                RecipeContext::new(
                    &failing_source,
                    &probe,
                    &fixture.app_state,
                    &clock,
                    &TomlSettingsStore::new(None),
                ),
                &session,
                id,
            )
            .expect("published refresh errors are acknowledged")
            .is_none()
        );

        let session = session.lock().expect("session lock");
        let tab = session.tab(id).expect("failed tab remains open");
        assert_eq!(tab.tab.label(), "Named refresh failure");
        assert!(matches!(tab.tab.state(), ViewerTabState::Error { .. }));
    }

    #[test]
    fn older_failing_refresh_becomes_stale_after_newer_success() {
        let source = source();
        let mut newest_view =
            compute_view(&source, &recipe(), &TomlSettingsStore::new(None)).expect("view computes");
        newest_view.title = "newer success".into();
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));
        let id = session.lock().expect("session").open(
            recipe(),
            "batch".into(),
            ViewerTabKind::Snapshot,
        );
        let (older, newer) = {
            let mut state = session.lock().expect("session");
            let older = state.begin_compute(id).expect("older");
            let newer = state.begin_compute(id).expect("newer");
            state.publish_if_current(newer, CachedView::new(Arc::new(newest_view)));
            (older, newer)
        };

        assert_eq!(
            publish_compute_error(&session, older, "sentinel /internal/sql"),
            Err(RecipeError::Stale)
        );
        let mut state = session.lock().expect("session");
        assert_eq!(state.current_ticket(id), Some(newer));
        assert_eq!(
            state.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Ready
        );
        assert_eq!(
            state.cached_view(id).expect("newer view").view.title,
            "newer success"
        );
    }
}
