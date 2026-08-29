use std::fmt::Write as _;

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
    pub remote_url: Option<RemoteUrl>,
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
        remote_url: target
            .remote_url
            .map(RemoteUrl::try_new)
            .transpose()
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
        remote_url: target.remote_url.as_ref().map(ToString::to_string),
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
    target.remote_url.as_ref().map_or_else(
        || target.remote.to_string(),
        |url| format!("{} ({url})", target.remote),
    )
}

#[must_use]
pub fn confirmation(command: &str, target: &PushTarget, message: &str) -> String {
    let remote = remote_label(target);
    let dest = format!("{}/{}", target.remote, target.branch);
    let PendingChanges {
        changed,
        staged,
        unprepared,
        ahead,
    } = target.pending;

    let mut out = format!(
        "{command}: review before committing & pushing:\n  repo:    {} ({})\n  branch:  {}\n  remote:  {}\n  message: {}\n\n{command} will:",
        target.name,
        target.top.display(),
        target.branch,
        remote,
        message
    );

    if !changed.is_zero() {
        if !unprepared.is_zero() {
            let _ = write!(
                out,
                "\n  • stage {unprepared} unprepared change(s) with `git add -A`"
            );
        }
        let staged_note = if staged.is_zero() {
            String::new()
        } else {
            format!(" ({staged} already staged)")
        };
        let _ = write!(
            out,
            "\n  • commit {changed} change(s){staged_note} as a single commit"
        );
        let pushed_count = ahead.into_inner().saturating_add(1);
        let _ = write!(out, "\n  • push {pushed_count} commit(s) to {dest}");
    } else if ahead != CommitCount::default() {
        let _ = write!(
            out,
            "\n  • nothing to commit; push {ahead} unpushed commit(s) to {dest}"
        );
    } else {
        out.push_str("\n  • nothing to commit or push, already up to date");
    }

    out
}

#[must_use]
pub fn push_confirmation(target: &PushTarget) -> String {
    let remote = remote_label(target);
    let dest = format!("{}/{}", target.remote, target.branch);
    let PendingChanges { changed, ahead, .. } = target.pending;

    let mut out = format!(
        "push: review before pushing:\n  repo:    {} ({})\n  branch:  {}\n  remote:  {}\n\npush will:",
        target.name,
        target.top.display(),
        target.branch,
        remote
    );

    if !changed.is_zero() {
        let _ = write!(
            out,
            "\n  • refuse to push while {changed} uncommitted change(s) are present"
        );
        out.push_str("\n  • push only existing commits; it will not stage or create a commit");
    } else if ahead != CommitCount::default() {
        let _ = write!(out, "\n  • push {ahead} unpushed commit(s) to {dest}");
    } else {
        out.push_str("\n  • nothing to push, already up to date");
    }

    out
}

#[must_use]
pub fn commit_confirmation(target: &CommitTarget, message: &str) -> String {
    let PendingChanges {
        changed,
        staged,
        unprepared,
        ahead: _,
    } = target.pending;

    let mut out = format!(
        "commit: review before committing:\n  repo:    {} ({})\n  branch:  {}\n  message: {}\n\ncommit will:",
        target.name,
        target.top.display(),
        target.branch,
        message
    );

    if changed.is_zero() {
        out.push_str("\n  • nothing to commit, working tree clean");
    } else {
        if !unprepared.is_zero() {
            let _ = write!(
                out,
                "\n  • stage {unprepared} unprepared change(s) with `git add -A`"
            );
        }
        let staged_note = if staged.is_zero() {
            String::new()
        } else {
            format!(" ({staged} already staged)")
        };
        let _ = write!(
            out,
            "\n  • commit {changed} change(s){staged_note} as a single commit"
        );
        out.push_str("\n  • leave the commit local; it will not push");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        branch_name, commit_count, path_count, project_name, remote_name, remote_url,
        repository_root,
    };

    fn push_target() -> PushTarget {
        PushTarget {
            name: project_name("repo-a"),
            top: repository_root("/home/me/work/repo-a"),
            branch: branch_name("main"),
            remote: remote_name("origin"),
            remote_url: Some(remote_url("git@github.com:me/repo-a.git")),
            pending: PendingChanges {
                changed: path_count(2),
                staged: path_count(0),
                unprepared: path_count(2),
                ahead: commit_count(2),
            },
        }
    }

    fn commit_target() -> CommitTarget {
        CommitTarget {
            name: project_name("repo-a"),
            top: repository_root("/home/me/work/repo-a"),
            branch: branch_name("main"),
            pending: PendingChanges {
                changed: path_count(3),
                staged: path_count(1),
                unprepared: path_count(2),
                ahead: commit_count(0),
            },
        }
    }

    #[test]
    fn confirmation_names_repo_branch_remote_and_message() {
        let text = confirmation("push", &push_target(), "save work");
        assert!(text.contains("repo-a"), "names the repo");
        assert!(text.contains("/home/me/work/repo-a"), "shows the path");
        assert!(text.contains("main"), "names the branch");
        assert!(text.contains("origin"), "names the remote");
        assert!(
            text.contains("git@github.com:me/repo-a.git"),
            "shows the remote url"
        );
        assert!(text.contains("save work"), "shows the commit message");
    }

    #[test]
    fn confirmation_omits_empty_remote_url_parens() {
        let mut target = push_target();
        target.remote_url = None;
        let text = confirmation("push", &target, "save work");
        assert!(text.contains("origin"));
        let remote_line = text
            .lines()
            .find(|line| line.trim_start().starts_with("remote:"))
            .unwrap();
        assert!(
            !remote_line.contains("()"),
            "no empty parens when url is unknown"
        );
    }

    #[test]
    fn confirmation_spells_out_stage_commit_push_for_a_dirty_repo() {
        let mut target = push_target();
        target.pending = PendingChanges {
            changed: path_count(3),
            staged: path_count(1),
            unprepared: path_count(2),
            ahead: commit_count(1),
        };
        let text = confirmation("push", &target, "save work");
        assert!(text.contains("stage 2 unprepared change(s)"), "{text}");
        assert!(
            text.contains("commit 3 change(s) (1 already staged) as a single commit"),
            "{text}"
        );
        assert!(text.contains("push 2 commit(s) to origin/main"), "{text}");
    }

    #[test]
    fn confirmation_clean_but_ahead_says_push_only() {
        let mut target = push_target();
        target.pending = PendingChanges {
            ahead: commit_count(3),
            ..PendingChanges::default()
        };
        let text = confirmation("push", &target, "ignored");
        assert!(
            text.contains("nothing to commit; push 3 unpushed commit(s) to origin/main"),
            "{text}"
        );
    }

    #[test]
    fn confirmation_clean_and_up_to_date_says_nothing_to_do() {
        let mut target = push_target();
        target.pending = PendingChanges::default();
        let text = confirmation("push", &target, "ignored");
        assert!(text.contains("already up to date"), "{text}");
    }

    #[test]
    fn push_only_confirmation_omits_message_and_commit_language() {
        let mut target = push_target();
        target.pending = PendingChanges {
            ahead: commit_count(3),
            ..PendingChanges::default()
        };

        let text = push_confirmation(&target);

        assert!(!text.contains("message:"), "{text}");
        assert!(!text.contains("committing & pushing"), "{text}");
        assert!(!text.contains("commit "), "{text}");
        assert!(text.contains("review before pushing"), "{text}");
        assert!(
            text.contains("push 3 unpushed commit(s) to origin/main"),
            "{text}"
        );
    }

    #[test]
    fn commit_confirmation_spells_out_local_only_commit() {
        let text = commit_confirmation(&commit_target(), "save work");

        assert!(text.contains("review before committing"), "{text}");
        assert!(text.contains("save work"), "{text}");
        assert!(text.contains("stage 2 unprepared change(s)"), "{text}");
        assert!(
            text.contains("commit 3 change(s) (1 already staged) as a single commit"),
            "{text}"
        );
        assert!(text.contains("leave the commit local"), "{text}");
        assert!(!text.contains("push "), "{text}");
    }
}
