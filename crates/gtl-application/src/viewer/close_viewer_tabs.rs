use gtl_models::{
    failure::{ErrorMeta, Failure, Resource},
    viewer::ViewerTabId,
};

use crate::viewer::{
    ViewerState, ViewerStateError,
    work::{self, ReserveRecipeError, ReservedRecipeWork},
};

#[derive(Clone, Copy)]
pub enum CloseViewerTabs {
    One(ViewerTabId),
    Others(ViewerTabId),
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum CloseViewerTabsError {
    #[error("viewer tab is not available")]
    #[meta(failure = Failure::Gone { resource: Resource::ViewerTab })]
    UnknownTab,
    #[error(transparent)]
    #[meta(transparent)]
    ViewerState(#[from] ViewerStateError),
    #[error(transparent)]
    #[meta(transparent)]
    ReserveWork(#[from] ReserveRecipeError),
}

/// Closes unpinned tabs and reserves work for a newly active tab that needs its view.
#[cqrsy::command]
pub fn execute(
    request: CloseViewerTabs,
    viewer_state: &ViewerState,
) -> Result<Option<ReservedRecipeWork>, CloseViewerTabsError> {
    viewer_state.update(|session| {
        let (CloseViewerTabs::One(tab_id) | CloseViewerTabs::Others(tab_id)) = request;
        if session.tab(tab_id).is_none() {
            return Err(CloseViewerTabsError::UnknownTab);
        }
        let ids = session
            .tabs()
            .filter(|tab| {
                !tab.pinned
                    && match request {
                        CloseViewerTabs::One(id) => tab.tab.id() == id,
                        CloseViewerTabs::Others(id) => tab.tab.id() != id,
                    }
            })
            .map(|tab| tab.tab.id())
            .collect::<Vec<_>>();
        let active = session.active();
        for id in ids {
            session.close(id);
        }
        if session.active() == active {
            Ok(None)
        } else {
            work::reserve_active_if_needed(session).map_err(Into::into)
        }
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::ViewerTabId;

    use super::CloseViewerTabs;
    use crate::{
        recipes::{RecipeOp, RecipeTarget},
        utils,
        viewer::{ViewerState, close_viewer_tabs, work},
    };

    fn open_tab(viewer: &ViewerState, target: RecipeTarget) -> ViewerTabId {
        work::reserve_open(
            viewer,
            utils::viewer::recipe(RecipeOp::Diff { target }),
            gtl_models::recipes::RecipeBatchId::generate(),
        )
        .unwrap()
        .ticket()
        .tab_id
    }

    fn unpushed() -> RecipeTarget {
        RecipeTarget::Unpushed { pinned: None }
    }

    fn working_tree() -> RecipeTarget {
        RecipeTarget::Base {
            rev: gtl_models::git::GitRevision::head(),
        }
    }

    #[test]
    fn closing_others_keeps_pinned_tabs() {
        let viewer = ViewerState::new();
        let pinned = open_tab(&viewer, unpushed());
        let other = open_tab(&viewer, working_tree());
        viewer
            .update(|session| session.set_pinned(pinned, true))
            .unwrap();

        close_viewer_tabs::execute(CloseViewerTabs::Others(pinned), &viewer).unwrap();
        close_viewer_tabs::execute(CloseViewerTabs::One(pinned), &viewer).unwrap();

        viewer
            .inspect(|session| {
                assert!(session.tab(other).is_none());
                assert!(session.tab(pinned).is_some());
            })
            .unwrap();
    }

    #[test]
    fn closing_tabs_refreshes_only_when_the_active_tab_changes() {
        let viewer = ViewerState::new();
        let first = open_tab(&viewer, unpushed());
        let second = open_tab(&viewer, working_tree());
        let refresh = close_viewer_tabs::execute(CloseViewerTabs::One(second), &viewer)
            .unwrap()
            .unwrap();
        assert_eq!(refresh.ticket().tab_id, first);
        let second = open_tab(&viewer, working_tree());
        assert!(
            close_viewer_tabs::execute(CloseViewerTabs::One(first), &viewer)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            viewer.inspect(|session| session.active()).unwrap(),
            Some(second)
        );
    }
}
