use gtl_models::failure::{ErrorMeta, Failure, Resource};
use gtl_wire::viewer::SetViewerTabLive;

use super::{ViewerState, ViewerStateError};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum SetViewerTabLiveError {
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error("viewer tab is not available")]
    #[meta(failure = Failure::Gone { resource: Resource::ViewerTab })]
    UnknownTab,
}

/// Makes a tab follow its source, or keeps its current snapshot from then on.
///
/// A tab that goes live updates on the next live check unless its content already reflects the
/// source's current state.
#[cqrsy::command]
pub fn execute(
    request: SetViewerTabLive,
    state: &ViewerState,
) -> Result<(), SetViewerTabLiveError> {
    if state.update(|session| session.set_live(request.tab_id, request.live))? {
        Ok(())
    } else {
        Err(SetViewerTabLiveError::UnknownTab)
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{recipes::RecipeBatchId, viewer::ViewerTabId};

    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeTarget},
        utils,
        viewer::{set_viewer_tab_live, work},
    };

    #[test]
    fn a_tab_goes_live_and_back_without_a_new_computation() {
        let state = ViewerState::new();
        let ticket = work::reserve_open(
            &state,
            utils::viewer::recipe(RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            }),
            RecipeBatchId::generate(),
        )
        .unwrap()
        .ticket();

        for live in [true, false] {
            set_viewer_tab_live::execute(
                SetViewerTabLive {
                    tab_id: ticket.tab_id,
                    live,
                },
                &state,
            )
            .unwrap();
            state
                .inspect(|session| {
                    assert_eq!(session.tab(ticket.tab_id).unwrap().tab.live(), live);
                    assert_eq!(session.current_ticket(ticket.tab_id), Some(ticket));
                })
                .unwrap();
        }
    }

    #[test]
    fn an_unknown_tab_is_rejected() {
        let result = set_viewer_tab_live::execute(
            SetViewerTabLive {
                tab_id: ViewerTabId::try_new(9).unwrap(),
                live: true,
            },
            &ViewerState::new(),
        );

        assert!(matches!(result, Err(SetViewerTabLiveError::UnknownTab)));
    }
}
