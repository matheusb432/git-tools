//! Recipe compute orchestration through the desktop mediator.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use application::{
    diffs::{
        compute_diff::ComputeDiff, compute_merge_diff::ComputeMergeDiff,
        compute_squash_preview::ComputeSquashPreview,
    },
    history::record_render::RecordRender,
    live_views::probe::{ProbeOutcome, ProbeSource},
};
use cqrsy::{Handle, Sender};
use domain::{
    diffs::{DiffTarget, View},
    viewer::{ViewerTabId, ViewerTabKind, ViewerTabState},
};
use gtl_recipe::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

use crate::session::{CachedView, ComputeTicket, PublishOutcome, ViewerSession};

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
pub(crate) struct RefreshedRecipe {
    pub(crate) ticket: ComputeTicket,
    pub(crate) view: Option<Arc<View>>,
}

fn diff_target(target: &RecipeTarget) -> DiffTarget {
    match target {
        RecipeTarget::Unpushed => DiffTarget::Unpushed,
        RecipeTarget::Base { rev } => DiffTarget::Base(rev.clone()),
        RecipeTarget::Range { range } => DiffTarget::Range(range.clone()),
        RecipeTarget::Merge { base } => DiffTarget::Merge(base.clone()),
        RecipeTarget::Last { count } => DiffTarget::Last(*count),
    }
}

pub(crate) fn compute_view<M>(mediator: &M, recipe: &Recipe) -> Result<View, String>
where
    M: Sender<ComputeDiff> + Sender<ComputeMergeDiff> + Sender<ComputeSquashPreview> + Handle,
{
    let cwd = recipe.cwd();
    match &recipe.op {
        RecipeOp::Diff { target } => mediator
            .send_now(ComputeDiff {
                cwd,
                target: diff_target(target),
            })
            .map(|response| response.view)
            .map_err(|error| format!("{error:#}")),
        RecipeOp::MergeDiff { base } => mediator
            .send_now(ComputeMergeDiff {
                cwd,
                base: base.clone(),
            })
            .map(|response| response.view)
            .map_err(|error| format!("{error:#}")),
        RecipeOp::SquashPreview => mediator
            .send_now(ComputeSquashPreview { cwd })
            .map(|response| response.view)
            .map_err(|error| format!("{error:#}")),
    }
}

pub(crate) fn open_recipe<M>(
    mediator: &M,
    session: &Mutex<ViewerSession>,
    data_root: &Path,
    recipe: &Recipe,
    batch_id: String,
    kind: ViewerTabKind,
) -> Result<OpenedRecipe, RecipeError>
where
    M: Sender<ComputeDiff>
        + Sender<ComputeMergeDiff>
        + Sender<ComputeSquashPreview>
        + Sender<ProbeSource>
        + Sender<RecordRender>
        + Handle,
{
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

    let view = compute_and_publish(mediator, session, data_root, recipe, kind, ticket)?;
    Ok(OpenedRecipe {
        tab_id: id,
        ticket,
        view,
    })
}

#[cfg(test)]
pub(crate) fn refresh_recipe<M>(
    mediator: &M,
    session: &Mutex<ViewerSession>,
    data_root: &Path,
    id: ViewerTabId,
) -> Result<Option<Arc<View>>, String>
where
    M: Sender<ComputeDiff>
        + Sender<ComputeMergeDiff>
        + Sender<ComputeSquashPreview>
        + Sender<ProbeSource>
        + Sender<RecordRender>
        + Handle,
{
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

    compute_and_publish(mediator, session, data_root, &recipe, kind, ticket)
        .map_err(|error| error.to_string())
}

pub(crate) fn refresh_recipe_versioned<M>(
    mediator: &M,
    session: &Mutex<ViewerSession>,
    data_root: &Path,
    id: ViewerTabId,
) -> Result<RefreshedRecipe, RecipeError>
where
    M: Sender<ComputeDiff>
        + Sender<ComputeMergeDiff>
        + Sender<ComputeSquashPreview>
        + Sender<ProbeSource>
        + Sender<RecordRender>
        + Handle,
{
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
    let view = compute_and_publish(mediator, session, data_root, &recipe, kind, ticket)?;
    Ok(RefreshedRecipe { ticket, view })
}

fn compute_and_publish<M>(
    mediator: &M,
    session: &Mutex<ViewerSession>,
    data_root: &Path,
    recipe: &Recipe,
    kind: ViewerTabKind,
    ticket: ComputeTicket,
) -> Result<Option<Arc<View>>, RecipeError>
where
    M: Sender<ComputeDiff>
        + Sender<ComputeMergeDiff>
        + Sender<ComputeSquashPreview>
        + Sender<ProbeSource>
        + Sender<RecordRender>
        + Handle,
{
    if kind == ViewerTabKind::Live
        && let Some(broken) = probe_live_source(mediator, data_root, recipe)?
    {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        if session.set_state_if_current(ticket, broken) == PublishOutcome::Stale {
            return Err(RecipeError::Stale);
        }
        return Ok(None);
    }

    let view = match compute_view(mediator, recipe) {
        Ok(view) => Arc::new(view),
        Err(reason) => {
            publish_compute_error(session, ticket, &reason)?;
            return Err(RecipeError::Failed(reason));
        }
    };
    let published = {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        session.publish_if_current(ticket, CachedView::new(Arc::clone(&view)))
    };
    if published == PublishOutcome::Published {
        record_render(mediator, data_root, recipe, &view);
        return Ok(Some(view));
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

fn probe_live_source<M>(
    mediator: &M,
    data_root: &Path,
    recipe: &Recipe,
) -> Result<Option<ViewerTabState>, String>
where
    M: Sender<ProbeSource> + Handle,
{
    let RecipeSource::LocalRepo(path) = &recipe.source;
    let response = mediator
        .send_now(ProbeSource {
            data_root: data_root.to_path_buf(),
            source_kind: "LocalRepo".into(),
            source_value: path.display().to_string(),
        })
        .map_err(|error| format!("{error:#}"))?;
    Ok(match response.outcome {
        ProbeOutcome::Ok => None,
        ProbeOutcome::Broken { rejection } => Some(ViewerTabState::Broken {
            code: rejection.code().into(),
            reason: rejection.to_string(),
        }),
    })
}

fn record_render<M>(mediator: &M, data_root: &Path, recipe: &Recipe, view: &View)
where
    M: Sender<RecordRender> + Handle,
{
    let Ok(recipe_json) = serde_json::to_string(recipe) else {
        return;
    };
    if let Err(error) = mediator.send_now(RecordRender {
        data_root: data_root.to_path_buf(),
        recipe_json,
        title: view.title.clone(),
        repo_name: view.repo_name.clone(),
        kind: recipe.kind_tag().into(),
        range_label: view.cmd.range.clone(),
    }) {
        eprintln!("gtl-viewer: failed to record render history: {error:#}");
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Mutex};

    use application::{
        ports::RepoProbeResult,
        testing::{FakeDiffSource, FakeRepoProbe, InMemoryAppStateStore},
    };
    use domain::{
        diffs::Commit,
        viewer::{ViewerTabKind, ViewerTabState},
    };
    use gtl_recipe::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

    use super::*;
    use crate::session::ViewerSession;

    const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1 @@\n\
-old\n\
+new\n";

    fn recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo("/repo".into()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed,
            },
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

    #[test]
    fn open_computes_publishes_and_records_only_the_current_view() {
        let app_state = InMemoryAppStateStore::default();
        let mediator = crate::test_support::fake_mediator_with(
            source(),
            app_state.clone(),
            FakeRepoProbe {
                result: RepoProbeResult::Repo {
                    top_level: "/repo".into(),
                },
            },
        );
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));

        let opened = open_recipe(
            &mediator,
            &session,
            Path::new("/data"),
            &recipe(),
            "batch-1".into(),
            ViewerTabKind::Snapshot,
        )
        .expect("open succeeds");
        let id = opened.tab_id;
        assert!(opened.view.is_some());

        let mut session = session.lock().expect("session lock");
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Ready
        );
        assert_eq!(session.cached_view(id).expect("view").view.files.len(), 1);
        drop(session);
        let renders = app_state.renders.lock().expect("renders lock");
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].kind, "diff");
    }

    #[test]
    fn oversize_open_returns_the_view_without_retaining_it() {
        let mediator = crate::test_support::fake_mediator_with(
            source(),
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        );
        let session = Mutex::new(ViewerSession::new(1));

        let opened = open_recipe(
            &mediator,
            &session,
            Path::new("/data"),
            &recipe(),
            "batch".into(),
            ViewerTabKind::Snapshot,
        )
        .expect("open succeeds");

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
        let app_state = InMemoryAppStateStore::default();
        let mediator = crate::test_support::fake_mediator_with(
            FakeDiffSource::default(),
            app_state.clone(),
            FakeRepoProbe {
                result: RepoProbeResult::NotFound,
            },
        );
        let session = Mutex::new(ViewerSession::new(1024));

        let id = open_recipe(
            &mediator,
            &session,
            Path::new("/data"),
            &recipe(),
            "live".into(),
            ViewerTabKind::Live,
        )
        .expect("broken source remains an open tab")
        .tab_id;

        let session = session.lock().expect("session lock");
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Broken {
                code: "DirNotFound".into(),
                reason: "The git repo's directory at `/repo` was not found.".into(),
            }
        );
        assert!(app_state.renders.lock().expect("renders lock").is_empty());
    }

    #[test]
    fn non_repo_live_source_is_published_without_invoking_diff_compute() {
        let app_state = InMemoryAppStateStore::default();
        let mediator = crate::test_support::fake_mediator_with(
            FakeDiffSource::default(),
            app_state.clone(),
            FakeRepoProbe {
                result: RepoProbeResult::NotAGitRepo,
            },
        );
        let session = Mutex::new(ViewerSession::new(1024));

        let id = open_recipe(
            &mediator,
            &session,
            Path::new("/data"),
            &recipe(),
            "live".into(),
            ViewerTabKind::Live,
        )
        .expect("broken source remains an open tab")
        .tab_id;

        let session = session.lock().expect("session lock");
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Broken {
                code: "DirNotGitRepo".into(),
                reason: "The directory `/repo` is not a git repository.".into(),
            }
        );
        assert!(app_state.renders.lock().expect("renders lock").is_empty());
    }

    #[test]
    fn refresh_after_snapshot_to_live_reopen_probes_before_compute() {
        let app_state = InMemoryAppStateStore::default();
        let recipe = recipe();
        let mediator = crate::test_support::fake_mediator_with(
            source(),
            app_state.clone(),
            FakeRepoProbe {
                result: RepoProbeResult::NotFound,
            },
        );
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));
        let id = open_recipe(
            &mediator,
            &session,
            Path::new("/data"),
            &recipe,
            "snapshot".into(),
            ViewerTabKind::Snapshot,
        )
        .expect("snapshot computes without probing")
        .tab_id;
        {
            let mut session = session.lock().expect("session lock");
            assert_eq!(session.open(recipe, "live".into(), ViewerTabKind::Live), id);
        }
        app_state.renders.lock().expect("renders lock").clear();

        let refreshed = refresh_recipe(&mediator, &session, Path::new("/data"), id)
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
        assert!(app_state.renders.lock().expect("renders lock").is_empty());
    }

    #[test]
    fn refresh_recomputes_the_reserved_tab_and_records_the_current_result() {
        let app_state = InMemoryAppStateStore::default();
        let mediator = crate::test_support::fake_mediator_with(
            source(),
            app_state.clone(),
            FakeRepoProbe::default(),
        );
        let session = Mutex::new(ViewerSession::new(128 * 1024 * 1024));
        let id = open_recipe(
            &mediator,
            &session,
            Path::new("/data"),
            &recipe(),
            "batch".into(),
            ViewerTabKind::Snapshot,
        )
        .expect("open succeeds")
        .tab_id;
        app_state.renders.lock().expect("renders lock").clear();

        refresh_recipe(&mediator, &session, Path::new("/data"), id).expect("refresh succeeds");

        assert_eq!(app_state.renders.lock().expect("renders lock").len(), 1);
        let mut session = session.lock().expect("session lock");
        assert!(session.cached_view(id).is_some());
    }

    #[test]
    fn older_failing_refresh_becomes_stale_after_newer_success() {
        let mediator = crate::test_support::fake_mediator_with(
            source(),
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        );
        let mut newest_view = compute_view(&mediator, &recipe()).expect("view computes");
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
