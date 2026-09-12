use anyhow::Context as _;
use gtl_models::{
    git::{BranchName, CommitCount, RemoteName, RemoteUrl},
    paths::{ProjectName, RepositoryRoot},
    repository::{PathCount, PendingChanges},
};
use gtl_wire::v1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    pub name: ProjectName,
    pub top: RepositoryRoot,
    pub branch: BranchName,
    pub remote: RemoteName,
    pub remote_urls: Vec<RemoteUrl>,
    pub pending: PendingChanges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTarget {
    pub name: ProjectName,
    pub top: RepositoryRoot,
    pub branch: BranchName,
    pub pending: PendingChanges,
}

pub(crate) fn push_target_from_grpc(
    target: v1::RepositoryPushTarget,
) -> anyhow::Result<PushTarget> {
    Ok(PushTarget {
        name: ProjectName::try_new(target.project_name)
            .context("gtl-server returned an empty push project name")?,
        top: RepositoryRoot::try_new(target.repository_root.into())
            .context("gtl-server returned a non-absolute push repository root")?,
        branch: BranchName::try_new(target.branch)
            .context("gtl-server returned an empty push branch")?,
        remote: RemoteName::try_new(target.remote)
            .context("gtl-server returned an empty push remote")?,
        remote_urls: target
            .remote_urls
            .into_iter()
            .map(RemoteUrl::try_new)
            .collect::<Result<Vec<_>, _>>()
            .context("gtl-server returned an empty push remote URL")?,
        pending: pending_from_grpc(
            target
                .pending
                .context("gtl-server returned no pending push state")?,
        ),
    })
}

pub(crate) fn push_target_to_grpc(target: &PushTarget) -> v1::RepositoryPushTarget {
    v1::RepositoryPushTarget {
        project_name: target.name.to_string(),
        repository_root: target.top.to_string(),
        branch: target.branch.to_string(),
        remote: target.remote.to_string(),
        remote_urls: target.remote_urls.iter().map(ToString::to_string).collect(),
        pending: Some(pending_to_grpc(target.pending)),
    }
}

pub(crate) fn commit_target_from_grpc(
    target: v1::RepositoryCommitTarget,
) -> anyhow::Result<CommitTarget> {
    Ok(CommitTarget {
        name: ProjectName::try_new(target.project_name)
            .context("gtl-server returned an empty commit project name")?,
        top: RepositoryRoot::try_new(target.repository_root.into())
            .context("gtl-server returned a non-absolute commit repository root")?,
        branch: BranchName::try_new(target.branch)
            .context("gtl-server returned an empty commit branch")?,
        pending: pending_from_grpc(
            target
                .pending
                .context("gtl-server returned no pending commit state")?,
        ),
    })
}

pub(crate) fn commit_target_to_grpc(target: &CommitTarget) -> v1::RepositoryCommitTarget {
    v1::RepositoryCommitTarget {
        project_name: target.name.to_string(),
        repository_root: target.top.to_string(),
        branch: target.branch.to_string(),
        pending: Some(pending_to_grpc(target.pending)),
    }
}

fn pending_from_grpc(pending: v1::PendingChanges) -> PendingChanges {
    PendingChanges {
        changed: PathCount::new(pending.changed_paths),
        staged: PathCount::new(pending.staged_paths),
        unprepared: PathCount::new(pending.unprepared_paths),
        ahead: CommitCount::new(pending.commits_ahead),
    }
}

fn pending_to_grpc(pending: PendingChanges) -> v1::PendingChanges {
    v1::PendingChanges {
        changed_paths: pending.changed.value(),
        staged_paths: pending.staged.value(),
        unprepared_paths: pending.unprepared.value(),
        commits_ahead: pending.ahead.into_inner(),
    }
}

fn remote_label(target: &PushTarget) -> String {
    if target.remote_urls.is_empty() {
        target.remote.to_string()
    } else {
        format!(
            "{} ({})",
            target.remote,
            target
                .remote_urls
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

pub(crate) fn confirmation(target: &PushTarget) -> crate::confirm::Dialog {
    use crate::confirm::{Detail, Dialog};

    if target.pending.changed.is_zero() {
        return push_confirmation(target);
    }
    let details = vec![
        Detail::new("Project", &target.name),
        Detail::new("Branch", &target.branch),
        Detail::new("Files to commit", files_to_commit(target.pending)),
        Detail::new(
            "Commits to push",
            target.pending.ahead.into_inner().saturating_add(1),
        ),
        Detail::new("Push", remote_label(target)),
    ];
    Dialog::new(
        "Confirm commit and push",
        details,
        "Commit all changes and push?",
    )
}

pub(crate) fn push_confirmation(target: &PushTarget) -> crate::confirm::Dialog {
    use crate::confirm::{Detail, Dialog};

    Dialog::new(
        "Confirm push",
        vec![
            Detail::new("Project", &target.name),
            Detail::new("Branch", &target.branch),
            Detail::new("Commits to push", target.pending.ahead),
            Detail::new("Push", remote_label(target)),
        ],
        "Push these commits?",
    )
}

pub(crate) fn commit_confirmation(target: &CommitTarget) -> crate::confirm::Dialog {
    use crate::confirm::{Detail, Dialog};

    Dialog::new(
        "Confirm commit",
        vec![
            Detail::new("Project", &target.name),
            Detail::new("Branch", &target.branch),
            Detail::new("Files to commit", files_to_commit(target.pending)),
        ],
        "Commit all changes?",
    )
}

fn files_to_commit(pending: PendingChanges) -> String {
    format!(
        "{} (all changes, including unstaged and untracked)",
        pending.changed
    )
}

pub(crate) fn print_failure_progress(progress: Option<&v1::RepositoryMutationProgress>) {
    let Some(progress) = progress else {
        return;
    };
    match failure_progress(progress) {
        Ok(detail) if !detail.is_empty() => eprintln!("{detail}"),
        Ok(_) => {}
        Err(error) => eprintln!("operation progress: {error}"),
    }
}

fn failure_progress(progress: &v1::RepositoryMutationProgress) -> anyhow::Result<String> {
    use gtl_models::diffs::{CommitId, CommitIdAbbreviation};
    use v1::repository_mutation_progress::State;

    let created = |id: &str| -> anyhow::Result<String> {
        let id = CommitId::try_from(id.to_string())?;
        Ok(format!(
            "  Created local commit: {}",
            id.abbreviated(CommitIdAbbreviation::SevenCharacters)
        ))
    };
    match progress
        .state
        .as_ref()
        .context("server omitted mutation progress")?
    {
        State::Unchanged(_) => Ok(String::new()),
        State::Staged(_) => Ok("  Changes remain staged".into()),
        State::CreatedCommitId(id) => created(id),
        State::PushAttempted(attempt) => {
            let local = attempt
                .created_commit_id
                .as_deref()
                .map(created)
                .transpose()?;
            Ok(local.map_or_else(
                || "  Push outcome unconfirmed".into(),
                |local| format!("{local}\n  Push outcome unconfirmed"),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        branch_name, commit_count, path_count, project_name, remote_name, remote_url,
        repository_root,
    };

    #[test]
    fn commit_and_push_review_names_the_scope_and_destination() {
        let target = PushTarget {
            name: project_name("example-project"),
            top: repository_root("/repos/example-project"),
            branch: branch_name("main"),
            remote: remote_name("origin"),
            remote_urls: vec![remote_url("git@example.invalid:team/example-project.git")],
            pending: PendingChanges {
                changed: path_count(3),
                staged: path_count(1),
                unprepared: path_count(2),
                ahead: commit_count(2),
            },
        };
        let text = confirmation(&target).render("Confirm commit and push", false);
        assert!(text.contains("3 (all changes, including unstaged and untracked)"));
        assert!(text.contains("Commits to push  3"));
        assert!(text.contains("origin (git@example.invalid:team/example-project.git)"));
        assert!(!text.contains("/repos"));
        assert!(!text.contains("message"));
    }
}
