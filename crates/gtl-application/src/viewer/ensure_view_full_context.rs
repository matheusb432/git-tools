use std::sync::Arc;

use gtl_models::viewer::RenderOptions;
use gtl_wire::viewer::ViewerViewIdentity;

use super::{ViewerState, ViewerStateError, shell};
use crate::{
    diffs::{
        FetchFullContextDiff, FullContextDiffState, FullContextDiffTransitionError, View,
        fetch_full_context_diff, fetch_full_context_diff::FetchFullContextDiffError,
    },
    ports::GitClient,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureViewFullContext {
    Active,
    Identity {
        identity: ViewerViewIdentity,
        render_options: RenderOptions,
    },
}

#[derive(Debug, Clone)]
pub enum EnsureViewFullContextOk {
    Ready(Arc<View>),
    Stale,
}

#[derive(Debug, thiserror::Error)]
pub enum EnsureViewFullContextError {
    #[error(transparent)]
    State(#[from] ViewerStateError),
    #[error(transparent)]
    Fetch(#[from] FetchFullContextDiffError),
    #[error(transparent)]
    Transition(#[from] FullContextDiffTransitionError),
}

pub struct ReservedFullContext {
    snapshot: super::session::ActiveContentSnapshot,
}

pub fn reserve(
    state: &ViewerState,
    options: RenderOptions,
) -> Result<Option<ReservedFullContext>, ViewerStateError> {
    if options.density() != gtl_models::viewer::DiffDensity::Full {
        return Ok(None);
    }
    state.inspect(|session| {
        session
            .reserve_full_context()
            .map(|snapshot| ReservedFullContext { snapshot })
    })
}

pub fn execute_reserved(
    work: ReservedFullContext,
    state: &ViewerState,
    git: &impl GitClient,
) -> Result<EnsureViewFullContextOk, EnsureViewFullContextError> {
    let ReservedFullContext { snapshot } = work;
    let current = state.inspect(super::session::ViewerSession::active_content_snapshot)?;
    if current.as_ref().is_none_or(|current| {
        current.identity() != snapshot.identity()
            || !Arc::ptr_eq(&current.shared_view(), &snapshot.shared_view())
    }) {
        return Ok(EnsureViewFullContextOk::Stale);
    }
    let result = prepare_source(&snapshot, state, git);
    if result.is_err() {
        state.update(|session| session.fail_full_context_if_current(snapshot.identity()))?;
    }
    result
}

/// Ensures that a cached viewer snapshot can render Full-density rows.
#[cqrsy::command]
pub fn execute(
    command: EnsureViewFullContext,
    state: &ViewerState,
    git: &impl GitClient,
) -> Result<EnsureViewFullContextOk, EnsureViewFullContextError> {
    let snapshot = match command {
        EnsureViewFullContext::Active => {
            state.inspect(super::session::ViewerSession::active_content_snapshot)?
        }
        EnsureViewFullContext::Identity {
            identity,
            render_options,
        } => shell::content_snapshot_for_identity(state, identity, render_options)?,
    };
    let Some(snapshot) = snapshot else {
        return Ok(EnsureViewFullContextOk::Stale);
    };
    prepare_source(&snapshot, state, git)
}

fn prepare_source(
    snapshot: &super::session::ActiveContentSnapshot,
    state: &ViewerState,
    git: &impl GitClient,
) -> Result<EnsureViewFullContextOk, EnsureViewFullContextError> {
    let expected = snapshot.shared_view();
    let source = match &expected.full_context {
        FullContextDiffState::Deferred(source) => source,
        FullContextDiffState::Unavailable | FullContextDiffState::Loaded => {
            return Ok(EnsureViewFullContextOk::Ready(expected));
        }
    };

    let request = FetchFullContextDiff::new(&expected.repo_root, source);
    let full_context = fetch_full_context_diff::execute(&request, git)?;
    let replacement = state.prepare_snapshot(Arc::new(
        (*expected).clone().with_full_context(full_context)?,
    ))?;
    let published = state.update(|session| {
        let published = session.replace_active_content_if_current(
            snapshot.identity(),
            &expected,
            replacement.clone(),
        );
        if published.is_some() {
            session.mark_shell_changed();
        }
        published
    })?;
    Ok(published.map_or(
        EnsureViewFullContextOk::Stale,
        EnsureViewFullContextOk::Ready,
    ))
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        recipes::RecipeBatchId,
        viewer::{DiffDensity, DiffLayout, RenderOptions},
    };

    use super::*;
    use crate::{
        diffs::{DiffTarget, compute_diff},
        recipes::{RecipeOp, RecipeTarget},
        utils::{
            FakeGitClient, FixedUserSettingsStore,
            diffs::{DIFF_SINGLE_FILE, commit},
            repository_root, settings_with_density,
            viewer::recipe,
        },
        viewer::{ensure_view_full_context, session::CachedView, shell},
    };

    fn git() -> FakeGitClient {
        FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            commits: vec![commit("abc1234")],
            diff_output: DIFF_SINGLE_FILE.into(),
            full_diff_output: format!("{DIFF_SINGLE_FILE} retained context\n"),
            ..Default::default()
        }
    }

    fn deferred_view(git: &FakeGitClient) -> View {
        compute_diff::execute(
            compute_diff::ComputeDiff {
                repo_root: repository_root("/repos/project"),
                target: DiffTarget::Unpushed { pinned: None },
            },
            &FixedUserSettingsStore::default(),
            git,
            &crate::utils::SavedExtensionFilters::default(),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap()
        .view
    }

    fn ready_state(view: View) -> (ViewerState, ViewerViewIdentity, Arc<View>) {
        let state = ViewerState::new();
        let options = RenderOptions::new(DiffLayout::Unified, DiffDensity::Full);
        let shared = Arc::new(view);
        let identity = state
            .update(|session| {
                let id = session
                    .open(
                        recipe(RecipeOp::Diff {
                            target: RecipeTarget::Unpushed { pinned: None },
                        }),
                        RecipeBatchId::generate(),
                    )
                    .unwrap();
                let ticket = session.begin_compute(id).unwrap();
                session.publish_labeled_if_current(
                    ticket,
                    CachedView::new(Arc::clone(&shared)),
                    crate::utils::viewer::label("ready"),
                );
                shell::identity_for(session.active_content_identity().unwrap(), options)
            })
            .unwrap();
        (state, identity, shared)
    }

    #[test]
    fn full_identity_fetches_and_reweights_the_cached_view_once()
    -> Result<(), Box<dyn std::error::Error>> {
        let git = git();
        let (state, identity, _) = ready_state(deferred_view(&git));
        let project = |density| shell_for(&state, density).active;
        let compact_before = project(DiffDensity::Compact);
        assert!(matches!(
            project(DiffDensity::Full),
            gtl_wire::viewer::ViewerActiveState::Ready { view }
                if view.row_source == gtl_wire::viewer::ViewerRowSourceState::Pending
                    && !view.files.is_empty() && view.commit_count > 0
        ));
        let weight_before = state
            .inspect(|session| {
                session
                    .cached_view_snapshot(identity.tab_id)
                    .unwrap()
                    .weight()
            })
            .unwrap();

        let loaded = ensure_view_full_context::execute(
            EnsureViewFullContext::Identity {
                identity,
                render_options: RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
            },
            &state,
            &git,
        )
        .unwrap();
        let loaded = match loaded {
            EnsureViewFullContextOk::Ready(view) => Some(view),
            EnsureViewFullContextOk::Stale => None,
        };
        assert!(loaded.is_some(), "full-context request became stale");
        let Some(loaded) = loaded else {
            return Err("full-context request became stale".into());
        };
        let weight_after = state
            .inspect(|session| {
                session
                    .cached_view_snapshot(identity.tab_id)
                    .unwrap()
                    .weight()
            })
            .unwrap();

        assert!(weight_after > weight_before);
        assert_eq!(compact_before, project(DiffDensity::Compact));
        let full = project(DiffDensity::Full);
        let gtl_wire::viewer::ViewerActiveState::Ready { view: active } = full else {
            return Err("full source must be ready after enrichment".into());
        };
        assert_eq!(
            active.content_id,
            crate::viewer::project_diff_view(
                &loaded,
                &loaded,
                identity,
                gtl_wire::viewer::ViewerCommitSelection::None,
            )
            .content_id
        );
        let gtl_wire::viewer::ViewerActiveState::Ready { view: compact } = compact_before else {
            return Err("compact source must be ready".into());
        };
        assert_ne!(active.content_id, compact.content_id);
        assert!(
            loaded.files[0]
                .full_lines
                .as_ref()
                .is_some_and(|lines| lines.iter().any(|line| line == " retained context"))
        );

        let cached = ensure_view_full_context::execute(
            EnsureViewFullContext::Identity {
                identity,
                render_options: RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
            },
            &state,
            &FakeGitClient {
                full_diff_output: "diff --git a/../invalid b/../invalid".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            matches!(cached, EnsureViewFullContextOk::Ready(view) if Arc::ptr_eq(&view, &loaded))
        );
        Ok(())
    }

    fn shell_for(state: &ViewerState, density: DiffDensity) -> gtl_wire::viewer::ViewerShell {
        state
            .inspect(|session| shell::project(session, &settings_with_density(density)).unwrap())
            .unwrap()
    }

    #[test]
    fn reserved_source_work_is_coalesced_and_notifies_when_ready()
    -> Result<(), Box<dyn std::error::Error>> {
        let git = git();
        let (state, _, _) = ready_state(deferred_view(&git));
        let options = RenderOptions::new(DiffLayout::Unified, DiffDensity::Full);
        let mut watch = state.subscribe();
        let work = reserve(&state, options).unwrap().unwrap();
        assert!(reserve(&state, options).unwrap().is_none());
        let before = shell_for(&state, DiffDensity::Full);
        let gtl_wire::viewer::ViewerActiveState::Ready { view: pending } = before.active else {
            return Err(
                "file and commit metadata must be available during source preparation".into(),
            );
        };
        assert_eq!(
            pending.row_source,
            gtl_wire::viewer::ViewerRowSourceState::Pending
        );
        assert!(!pending.files.is_empty());
        assert!(pending.commit_count > 0);
        assert!(!watch.has_changed().unwrap());
        assert!(matches!(
            execute_reserved(work, &state, &git).unwrap(),
            EnsureViewFullContextOk::Ready(_)
        ));
        assert!(watch.has_changed().unwrap());
        let version = *watch.borrow_and_update();
        let ready = shell_for(&state, DiffDensity::Full);
        assert_eq!(ready.version, version);
        assert!(
            matches!(ready.active, gtl_wire::viewer::ViewerActiveState::Ready { view }
            if view.row_source == gtl_wire::viewer::ViewerRowSourceState::Ready)
        );
        assert!(reserve(&state, options).unwrap().is_none());
        Ok(())
    }

    #[test]
    fn failed_source_keeps_metadata_and_retries_only_after_refresh() {
        let (state, identity, source) = ready_state(deferred_view(&git()));
        let options = RenderOptions::new(DiffLayout::Unified, DiffDensity::Full);
        let work = reserve(&state, options).unwrap().unwrap();
        let invalid = FakeGitClient {
            full_diff_output: "diff --git a/../invalid b/../invalid".into(),
            ..Default::default()
        };
        assert!(execute_reserved(work, &state, &invalid).is_err());
        let shell = shell_for(&state, DiffDensity::Full);
        assert!(
            matches!(shell.active, gtl_wire::viewer::ViewerActiveState::Ready { view }
            if view.row_source == gtl_wire::viewer::ViewerRowSourceState::Failed
                && !view.files.is_empty() && view.commit_count > 0)
        );
        assert!(reserve(&state, options).unwrap().is_none());
        state
            .update(|session| {
                let ticket = session.refresh(identity.tab_id).unwrap();
                session.publish_labeled_if_current(
                    ticket,
                    CachedView::new(source),
                    crate::utils::viewer::label("refreshed"),
                );
            })
            .unwrap();
        let retry = reserve(&state, options).unwrap().unwrap();
        assert!(matches!(
            execute_reserved(retry, &state, &git()).unwrap(),
            EnsureViewFullContextOk::Ready(_)
        ));
    }

    #[test]
    fn unavailable_source_returns_the_same_cached_view_without_fetching() {
        let git = git();
        let mut view = deferred_view(&git);
        view.full_context = FullContextDiffState::Unavailable;
        let (state, identity, expected) = ready_state(view);

        let cached = ensure_view_full_context::execute(
            EnsureViewFullContext::Identity {
                identity,
                render_options: RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
            },
            &state,
            &FakeGitClient {
                full_diff_output: "diff --git a/../invalid b/../invalid".into(),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(
            matches!(cached, EnsureViewFullContextOk::Ready(view) if Arc::ptr_eq(&view, &expected))
        );
    }
}
