use gtl_models::failure::PushFailure;
use gtl_wire::viewer::push::CreateViewerPush;

use super::{PushError, PushPlan, ViewerPushGit};
use crate::{
    ports::UserSettingsReader,
    viewer::{ViewerState, source},
};

pub(super) fn prepare(
    request: &CreateViewerPush,
    viewer: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl ViewerPushGit,
) -> Result<PushPlan, PushError> {
    let (path, selected, branch) = match request {
        CreateViewerPush::Project { path } => (path.clone(), None, None),
        CreateViewerPush::View { identity } => {
            let snapshot = source::current(*identity, viewer, settings)?;
            let view = snapshot.view();
            let commit = view
                .commits
                .first()
                .ok_or(PushFailure::NothingToPush)?
                .id
                .clone();
            let origin = view.origin.repository().ok_or(PushFailure::NothingToPush)?;
            (
                origin.root.clone(),
                Some(commit),
                Some(origin.branch.clone()),
            )
        }
    };
    let repository = git.inspect_push(&path)?;
    if branch.is_some_and(|branch| branch.branch() != Some(&repository.branch)) {
        return Err(PushFailure::CheckoutChanged {
            current: repository.branch,
        }
        .into());
    }
    let commit = selected.unwrap_or_else(|| repository.head.clone());
    if !git.contains_commit(&path, &commit, &repository.head)? {
        return Err(PushFailure::CommitRemoved { commit }.into());
    }
    let count = git.count_commits(&path, &repository.upstream, &commit)?;
    if count == 0 {
        return Err(PushFailure::NothingToPush.into());
    }
    Ok(PushPlan {
        path,
        project: None,
        no_confirmation: false,
        repository,
        commit,
        count,
    })
}
