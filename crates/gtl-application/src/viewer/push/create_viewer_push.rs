use gtl_wire::viewer::push::{CreateViewerPush, ViewerPushId};

use super::{PushError, ViewerPushGit, ViewerPushOperations, ViewerPushProject, plan::prepare};
use crate::{ports::UserSettingsReader, viewer::ViewerState};

#[cqrsy::command]
pub fn execute(
    request: &CreateViewerPush,
    operations: &ViewerPushOperations,
    viewer: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl ViewerPushGit,
    projects: &impl ViewerPushProject,
) -> Result<ViewerPushId, PushError> {
    operations.insert(
        prepare(request, viewer, settings, git).and_then(|mut plan| {
            plan.project = projects
                .project_name(&plan.path)
                .map_err(PushError::ProjectLookup)?;
            Ok(plan)
        }),
    )
}
