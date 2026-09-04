use gtl_wire::viewer::MoveViewerTab;

use super::{ViewerState, ViewerStateError};

#[derive(Debug, thiserror::Error)]
pub enum MoveViewerTabError {
    #[error(transparent)]
    State(#[from] ViewerStateError),
    #[error("viewer tab is not available")]
    UnknownTab,
}

pub fn execute(request: MoveViewerTab, state: &ViewerState) -> Result<(), MoveViewerTabError> {
    state.update(|session| {
        session
            .move_tab(request.tab_id, request.target_tab_id, request.placement)
            .ok_or(MoveViewerTabError::UnknownTab)?;
        Ok(())
    })?
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{ViewerTabId, ViewerTabPlacement};
    use gtl_wire::viewer::MoveViewerTab;

    use super::{MoveViewerTabError, execute};
    use crate::viewer::ViewerState;

    #[test]
    fn unknown_tab_is_rejected() {
        let request = MoveViewerTab {
            tab_id: ViewerTabId::try_new(1).unwrap(),
            target_tab_id: ViewerTabId::try_new(2).unwrap(),
            placement: ViewerTabPlacement::Before,
        };

        assert!(matches!(
            execute(request, &ViewerState::new()),
            Err(MoveViewerTabError::UnknownTab)
        ));
    }
}
