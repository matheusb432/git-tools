use std::fmt::Write as _;

use gtl_application::repositories::{plan_commit::CommitTarget, plan_push::PushTarget};
use gtl_models::{git::CommitCount, repository::PendingChanges};

fn remote_label(target: &PushTarget) -> String {
    target.remote_url.as_ref().map_or_else(
        || target.remote.to_string(),
        |url| format!("{} ({url})", target.remote),
    )
}

/// Builds the review block printed before the current-repo stage/commit/push flow runs.
/// Spells out every side effect so the user confirms an action, not just a repository.
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
        "{command} — review before committing & pushing:\n  repo:    {} ({})\n  branch:  {}\n  remote:  {}\n  message: {}\n\n{command} will:",
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
        out.push_str("\n  • nothing to commit or push — already up to date");
    }

    out
}

/// Builds the review block printed before a plain current-repo push.
pub fn push_confirmation(target: &PushTarget) -> String {
    let remote = remote_label(target);
    let dest = format!("{}/{}", target.remote, target.branch);
    let PendingChanges { changed, ahead, .. } = target.pending;

    let mut out = format!(
        "push — review before pushing:\n  repo:    {} ({})\n  branch:  {}\n  remote:  {}\n\npush will:",
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
        out.push_str("\n  • nothing to push — already up to date");
    }

    out
}

/// Builds the review block printed before the current-repo local commit flow runs.
pub fn commit_confirmation(target: &CommitTarget, message: &str) -> String {
    let PendingChanges {
        changed,
        staged,
        unprepared,
        ahead: _,
    } = target.pending;

    let mut out = format!(
        "commit — review before committing:\n  repo:    {} ({})\n  branch:  {}\n  message: {}\n\ncommit will:",
        target.name,
        target.top.display(),
        target.branch,
        message
    );

    if changed.is_zero() {
        out.push_str("\n  • nothing to commit — working tree clean");
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
            .expect("has a remote line");
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
