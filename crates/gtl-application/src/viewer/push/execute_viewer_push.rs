use super::{PushError, PushPlan, ViewerPushGit};

#[cqrsy::command]
pub fn execute(plan: &PushPlan, git: &impl ViewerPushGit) -> Result<(), PushError> {
    let current = git.inspect_push(&plan.path)?;
    if current.branch != plan.repository.branch {
        return Err(PushError::CheckoutChanged);
    }
    if current.remote != plan.repository.remote
        || current.destination != plan.repository.destination
        || current.url != plan.repository.url
    {
        return Err(PushError::DestinationChanged);
    }
    if !git.contains_commit(&plan.path, &plan.commit, &current.head)? {
        return Err(PushError::CommitRemoved);
    }
    git.push_commit(plan)
}
