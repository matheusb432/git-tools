//! Terminal presentation for the recursive push review.

use std::{fmt::Write as _, path::Path};

use gtl_models::repository::recursive_push::{Dest, RepoTarget};

pub(crate) fn confirmation(root: &Path, targets: &[RepoTarget]) -> String {
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
    use super::*;
    use crate::testing::{branch_name, project_name, remote_name, repository_root};

    #[test]
    fn confirmation_lists_push_destinations_and_skips() {
        let targets = vec![
            RepoTarget {
                path: repository_root("/repos/api"),
                label: project_name("api"),
                dest: Dest::Push {
                    branch: branch_name("main"),
                    remote: remote_name("origin"),
                },
            },
            RepoTarget {
                path: repository_root("/repos/web"),
                label: project_name("web"),
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
            path: repository_root("/repos/api"),
            label: project_name("api"),
            dest: Dest::Synced {
                branch: branch_name("main"),
                remote: remote_name("origin"),
            },
        }];
        let text = confirmation(Path::new("/repos"), &targets);
        assert!(
            text.contains("api  (main → origin, already synced)"),
            "{text}"
        );
    }
}
