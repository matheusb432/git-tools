use gtl_wire::viewer::push::{CreateViewerPush, ViewerPushId};

use super::{PushError, ViewerPushGit, ViewerPushOperations, plan::prepare};
use crate::{ports::UserSettingsReader, viewer::ViewerState};

#[cqrsy::command]
pub fn execute(
    request: &CreateViewerPush,
    operations: &ViewerPushOperations,
    viewer: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl ViewerPushGit,
) -> Result<ViewerPushId, PushError> {
    operations.insert(prepare(request, viewer, settings, git))
}
