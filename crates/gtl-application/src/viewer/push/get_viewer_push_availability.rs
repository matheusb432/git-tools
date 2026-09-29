use gtl_models::failure::{Classified, PushFailure};
use gtl_wire::viewer::{
    ViewerViewIdentity,
    push::{CreateViewerPush, ViewerPushAvailability, ViewerPushState},
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
) -> ViewerPushState {
    let availability =
        match plan::prepare(&CreateViewerPush::View { identity }, viewer, settings, git) {
            Ok(_) => ViewerPushAvailability::Available,
            Err(PushError::Refused(PushFailure::NothingToPush)) => {
                ViewerPushAvailability::NothingToPush
            }
            Err(error) => ViewerPushAvailability::Blocked {
                failure: error.classify().into_failure(),
            },
        };
    let snapshot_has_unpushed_commits = snapshot_unpushed(identity, viewer, settings, git).ok();
    ViewerPushState {
        availability,
        snapshot_has_unpushed_commits,
    }
}

fn snapshot_unpushed(
    identity: ViewerViewIdentity,
    viewer: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl ViewerPushGit,
) -> Result<bool, PushError> {
    crate::viewer::source::current(identity, viewer, settings)?;
    let snapshot = viewer
        .update(|session| session.cached_view_snapshot(identity.tab_id))
        .map_err(crate::viewer::source::ViewerSourceError::from)?
        .ok_or(crate::viewer::source::ViewerSourceError::Changed)?;
    let view = &snapshot.view;
    let Some(commit) = view.commits.first() else {
        return Ok(false);
    };
    let repository = git.inspect_push(&view.repo_root)?;
    Ok(git.count_commits(&view.repo_root, &repository.upstream, &commit.id)? > 0)
}
