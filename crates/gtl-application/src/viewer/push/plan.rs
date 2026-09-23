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
                .ok_or(PushError::NothingToPush)?
                .id
                .clone();
            (
                view.repo_root.clone(),
                Some(commit),
                Some(view.branch.clone()),
            )
        }
    };
    let repository = git.inspect_push(&path)?;
    if branch.is_some_and(|branch| branch.branch() != Some(&repository.branch)) {
        return Err(PushError::CheckoutChanged);
    }
    let commit = selected.unwrap_or_else(|| repository.head.clone());
    if !git.contains_commit(&path, &commit, &repository.head)? {
        return Err(PushError::CommitRemoved);
    }
    let count = git.count_commits(&path, &repository.upstream, &commit)?;
    if count == 0 {
        return Err(PushError::NothingToPush);
    }
    Ok(PushPlan {
        path,
        repository,
        commit,
        count,
    })
}
