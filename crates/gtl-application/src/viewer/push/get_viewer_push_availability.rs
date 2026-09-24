use gtl_models::failure::{Classified, PushFailure};
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
        Err(PushError::Refused(PushFailure::NothingToPush)) => {
            ViewerPushAvailability::NothingToPush
        }
        Err(error) => ViewerPushAvailability::Blocked {
            failure: error.classify().into_failure(),
        },
    }
}
