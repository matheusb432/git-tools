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
            let preferences = settings
                .load()
                .map_err(crate::viewer::source::ViewerSourceError::from)?;
            plan.no_confirmation = !preferences.viewer_push_confirmation_required()
                || plan.project.as_ref().is_some_and(|project| {
                    preferences
                        .viewer_push_no_confirmation_projects()
                        .contains(project)
                });
            Ok(plan)
        }),
    )
}
