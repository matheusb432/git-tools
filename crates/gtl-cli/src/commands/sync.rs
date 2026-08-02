use std::fmt::Write as _;

use gtl_application::repository_sync::{plan_commit::CommitTarget, plan_push::PushTarget};
use gtl_models::repository::PendingChanges;

/// Builds the review block printed before the current-repo stage/commit/push flow runs.
/// Spells out every side effect so the user confirms an action, not just a repository.
pub fn confirmation(command: &str, target: &PushTarget, message: &str) -> String {
    let remote = if target.remote_url.is_empty() {
        target.remote.clone()
    } else {
        format!("{} ({})", target.remote, target.remote_url)
    };
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

    if changed > 0 {
        if unprepared > 0 {
            let _ = write!(
                out,
                "\n  • stage {unprepared} unprepared change(s) with `git add -A`"
            );
        }
        let staged_note = if staged > 0 {
            format!(" ({staged} already staged)")
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "\n  • commit {changed} change(s){staged_note} as a single commit"
        );
        let _ = write!(out, "\n  • push {} commit(s) to {dest}", ahead + 1);
    } else if ahead > 0 {
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
    let remote = if target.remote_url.is_empty() {
        target.remote.clone()
    } else {
        format!("{} ({})", target.remote, target.remote_url)
    };
    let dest = format!("{}/{}", target.remote, target.branch);
    let PendingChanges { changed, ahead, .. } = target.pending;

    let mut out = format!(
        "push — review before pushing:\n  repo:    {} ({})\n  branch:  {}\n  remote:  {}\n\npush will:",
        target.name,
        target.top.display(),
        target.branch,
        remote
    );

    if changed > 0 {
        let _ = write!(
            out,
            "\n  • refuse to push while {changed} uncommitted change(s) are present"
        );
        out.push_str("\n  • push only existing commits; it will not stage or create a commit");
    } else if ahead > 0 {
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

    if changed > 0 {
        if unprepared > 0 {
            let _ = write!(
                out,
                "\n  • stage {unprepared} unprepared change(s) with `git add -A`"
            );
        }
        let staged_note = if staged > 0 {
            format!(" ({staged} already staged)")
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "\n  • commit {changed} change(s){staged_note} as a single commit"
        );
        out.push_str("\n  • leave the commit local; it will not push");
    } else {
        out.push_str("\n  • nothing to commit — working tree clean");
    }

    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn push_target() -> PushTarget {
        PushTarget {
            name: "repo-a".into(),
            top: PathBuf::from("/home/me/work/repo-a"),
            branch: "main".into(),
            remote: "origin".into(),
            remote_url: "git@github.com:me/repo-a.git".into(),
            pending: PendingChanges {
                changed: 2,
                staged: 0,
                unprepared: 2,
                ahead: 2,
            },
        }
    }

    fn commit_target() -> CommitTarget {
        CommitTarget {
            name: "repo-a".into(),
            top: PathBuf::from("/home/me/work/repo-a"),
            branch: "main".into(),
            pending: PendingChanges {
                changed: 3,
                staged: 1,
                unprepared: 2,
                ahead: 0,
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
        target.remote_url = String::new();
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
            changed: 3,
            staged: 1,
            unprepared: 2,
            ahead: 1,
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
            ahead: 3,
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
            ahead: 3,
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
