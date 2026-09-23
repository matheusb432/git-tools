use gtl_wire::viewer::{
    ViewerViewIdentity,
    push::{CreateViewerPush, ViewerPushAvailability},
};

use super::{PushError, ViewerPushGit, plan};
use crate::{ports::UserSettingsReader, viewer::ViewerState};

/// Current Git eligibility, independent of immutable snapshot contents.
#[cqrsy::query]
pub fn execute(
    identity: ViewerViewIdentity,
    viewer: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl ViewerPushGit,
) -> ViewerPushAvailability {
    match plan::prepare(&CreateViewerPush::View { identity }, viewer, settings, git) {
        Ok(_) => ViewerPushAvailability::Available,
        Err(PushError::NothingToPush) => ViewerPushAvailability::NothingToPush,
        Err(error) => ViewerPushAvailability::Unavailable {
            message: error.to_string(),
        },
    }
}
