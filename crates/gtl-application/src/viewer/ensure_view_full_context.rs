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
    let expected = snapshot.shared_view();
    let source = match &expected.full_context {
        FullContextDiffState::Deferred(source) => source,
        FullContextDiffState::Unavailable | FullContextDiffState::Loaded => {
            return Ok(EnsureViewFullContextOk::Ready(expected));
        }
    };

    let request = FetchFullContextDiff::new(&expected.repo_root, source);
    let full_context = fetch_full_context_diff::execute(&request, git)?;
    let replacement = Arc::new((*expected).clone().with_full_context(full_context)?);
    let published = state.inspect(|session| {
        session.replace_active_content_if_current(
            snapshot.identity(),
            &expected,
            Arc::clone(&replacement),
        )
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
        viewer::{DiffDensity, DiffLayout, RenderOptions, ViewerTabKind},
    };

    use super::*;
    use crate::{
        diffs::{DiffTarget, compute_diff},
        recipes::{RecipeOp, RecipeTarget},
        utils::{
            FakeGitClient, FixedUserSettingsStore,
            diffs::{DIFF_SINGLE_FILE, commit},
            repository_root,
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
                        ViewerTabKind::Snapshot,
                    )
                    .unwrap();
                let ticket = session.begin_compute(id).unwrap();
                session.publish_labeled_if_current(
                    ticket,
                    CachedView::new(Arc::clone(&shared)),
                    "ready".into(),
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
        let project = |density| {
            state
                .inspect(|session| {
                    shell::project(
                        session,
                        RenderOptions::new(DiffLayout::Unified, density),
                        super::super::Theme::Dark,
                        gtl_models::viewer::ViewerKeybindings::default(),
                        None,
                    )
                    .unwrap()
                })
                .unwrap()
                .active
        };
        let compact_before = project(DiffDensity::Compact);
        assert!(matches!(
            project(DiffDensity::Full),
            gtl_wire::viewer::ViewerActiveState::Pending { .. }
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
