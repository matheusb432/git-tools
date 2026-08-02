//! The `push_subrepos` feature slice behind recursive `push -r`: discover every
//! git repo under a root, resolve each one's push destination from *local* refs
//! only (no fetch), then push the pushable ones.
//!
//! [`plan`] is the read-only query (discover + inspect); [`apply`] is the command
//! that pushes a confirmed plan; [`confirmation`] renders the review block shown
//! between the two. A repo with no upstream — or a detached HEAD — carries a
//! [`Dest::Skip`](gtl_models::managed::push_subrepos::Dest) reason instead of a push
//! target, so an un-pushable repo is unrepresentable as a push and is reported,
//! never silently dropped. A branch already synced with its upstream (the local
//! `@{u}..HEAD` count is `0`, the same check `gtl status --all` reports) becomes a
//! `Dest::Synced`, so synced repos never reach the network.

use std::{fmt::Write as _, path::Path};

use gtl_models::managed::push_subrepos::{Dest, RepoTarget};

pub mod apply;
pub mod plan;

/// Builds the review block printed before any push. Lists every discovered repo and where
/// its current branch would land — or why it will be skipped — so the user confirms the
/// exact set, not just a count.
pub fn confirmation(root: &Path, targets: &[RepoTarget]) -> String {
    let mut out = format!(
        "push -r — push {} repo(s) under {}:",
        targets.len(),
        root.display()
    );
    for target in targets {
        match &target.dest {
            Dest::Push { branch, remote } => {
                let _ = write!(out, "\n  {}  ({branch} → {remote})", target.label);
            }
            Dest::Synced { branch, remote } => {
                let _ = write!(
                    out,
                    "\n  {}  ({branch} → {remote}, already synced)",
                    target.label
                );
            }
            Dest::Skip { reason } => {
                let _ = write!(out, "\n  {}  (skip — {reason})", target.label);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn confirmation_lists_push_destinations_and_skips() {
        let targets = vec![
            RepoTarget {
                path: PathBuf::from("/repos/api"),
                label: "api".into(),
                dest: Dest::Push {
                    branch: "main".into(),
                    remote: "origin".into(),
                },
            },
            RepoTarget {
                path: PathBuf::from("/repos/web"),
                label: "web".into(),
                dest: Dest::Skip {
                    reason: "no upstream tracking branch".into(),
                },
            },
        ];
        let text = confirmation(Path::new("/repos"), &targets);
        assert!(text.contains("push 2 repo(s) under /repos"), "{text}");
        assert!(text.contains("api  (main → origin)"), "{text}");
        assert!(
            text.contains("web  (skip — no upstream tracking branch)"),
            "{text}"
        );
    }

    #[test]
    fn confirmation_marks_already_synced_repos() {
        let targets = vec![RepoTarget {
            path: PathBuf::from("/repos/api"),
            label: "api".into(),
            dest: Dest::Synced {
                branch: "main".into(),
                remote: "origin".into(),
            },
        }];
        let text = confirmation(Path::new("/repos"), &targets);
        assert!(
            text.contains("api  (main → origin, already synced)"),
            "{text}"
        );
    }
}
