use std::path::PathBuf;

use gtl_models::{
    git::{CommitCount, GitEffectMode, GitRange, GitRevision, RemoteName},
    projects::ProjectRepository,
};

use super::resolve_repository_root::{self, ResolveRepositoryRootError};
use crate::{
    ports::{GitClient, GitEffect},
    projects::remote_sync::{self, Preflight, RepoSyncResult, SyncStatus},
};

pub struct PullRepository {
    pub path: PathBuf,
    pub mode: GitEffectMode,
}

#[cqrsy::command]
pub fn execute(
    request: PullRepository,
    git: &impl GitClient,
) -> Result<RepoSyncResult, ResolveRepositoryRootError> {
    let path = resolve_repository_root::execute(request.path, git)?;
    let repository = ProjectRepository {
        name: path.project_name(),
        path,
        remote: None,
    };
    Ok(pull_one(git, &repository, request.mode))
}

pub(crate) fn pull_one(
    git: &impl GitClient,
    repo: &ProjectRepository,
    mode: GitEffectMode,
) -> RepoSyncResult {
    let branch = match remote_sync::preflight(git, repo, "detached HEAD - nothing to pull onto") {
        Preflight::Done(result) => return result,
        Preflight::Ready { branch } => branch,
    };

    let remote = RemoteName::origin();
    match git.fetch(&repo.path, &remote) {
        Ok(GitEffect::Applied(_)) => {}
        Ok(GitEffect::Rejected(detail)) => {
            let detail = format!("fetch failed: {detail}");
            return remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &detail);
        }
        Err(error) => {
            return remote_sync::result(
                repo,
                Some(&branch),
                SyncStatus::Fail,
                &format!("fetch failed: {error}"),
            );
        }
    }

    let remote_branch = GitRevision::remote_branch(&remote, &branch);
    match git.revision_exists(&repo.path, &remote_branch) {
        Ok(true) => {}
        _ => {
            return remote_sync::result(
                repo,
                Some(&branch),
                SyncStatus::Warn,
                &format!("no '{branch}' branch on origin"),
            );
        }
    }

    let range = GitRange::three_dot(
        &GitRevision::remote_tracking(&remote, &branch),
        &GitRevision::from(&branch),
    );
    let Ok(Some(divergence)) = git.ahead_behind(&repo.path, &range) else {
        return remote_sync::result(repo, Some(&branch), SyncStatus::Fail, "rev-list failed");
    };
    let behind = divergence.behind;
    let ahead = divergence.ahead;

    if behind == CommitCount::default() {
        let detail = if ahead == CommitCount::default() {
            "up to date".to_string()
        } else {
            format!("up to date (local ahead by {ahead} - push pending)")
        };
        return remote_sync::result(repo, Some(&branch), SyncStatus::UpToDate, &detail);
    }
    if ahead != CommitCount::default() {
        let detail = format!("diverged (ahead {ahead}, behind {behind}) - resolve manually");
        return remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &detail);
    }
    if mode.is_dry_run() {
        let detail = format!("behind by {behind} - fast-forward");
        return remote_sync::result(repo, Some(&branch), SyncStatus::WouldPull, &detail);
    }

    let merge_ref = GitRevision::remote_tracking(&remote, &branch);
    match git.fast_forward(&repo.path, &merge_ref) {
        Ok(GitEffect::Applied(_)) => {
            let detail = format!(
                "fast-forwarded {behind} commit{}",
                if behind.into_inner() == 1 { "" } else { "s" }
            );
            remote_sync::result(repo, Some(&branch), SyncStatus::Pulled, &detail)
        }
        Ok(GitEffect::Rejected(detail)) => {
            let detail = detail
                .lines()
                .find(|line| line.starts_with("error:") || line.starts_with("fatal:"))
                .map_or("ff merge failed", str::trim)
                .to_string();
            remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &detail)
        }
        Err(error) => {
            remote_sync::result(repo, Some(&branch), SyncStatus::Fail, &error.to_string())
        }
    }
}
