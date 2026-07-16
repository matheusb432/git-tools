//! `prune` — delete local branches whose commits are already merged into `<onto>`
//! (default `main`). A branch qualifies iff it is an ancestor of `<onto>`; we list
//! candidates with `git for-each-ref --merged <onto>` and delete with `git branch -D`,
//! gated by that ancestor query. Plan/apply split mirrors [`crate::commands::sw`].

use std::{fmt::Write as _, path::Path};

use application::{
    ports::GitRunner,
    shared::git::{capture, onto_exists},
};

/// One prunable branch and the short sha it points at (captured for the recovery hint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub sha: String,
}

/// Read-only plan for the prune flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrunePlan {
    /// Cannot prune; the string explains why (stderr, exit 1).
    Refused(String),
    /// No branches qualify; nothing to do (stdout, exit 0).
    Nothing(String),
    /// Ready to delete: resolved repo top, target branch, and the qualifying branches.
    Ready {
        top: String,
        onto: String,
        branches: Vec<Branch>,
    },
}

/// Outcome of an applied prune.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Every branch deleted.
    Ok,
    /// Some branches deleted, some failed.
    Partial,
    /// No branch deleted (all failed).
    Fail,
}

/// A branch that could not be deleted, with git's reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedBranch {
    pub name: String,
    pub reason: String,
}

/// Applied-prune result: a status, a human-readable detail block, and the per-branch
/// outcomes (so both the single-repo and managed-fan-out callers can reuse it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneResult {
    pub status: Status,
    pub detail: String,
    pub deleted: Vec<Branch>,
    pub failed: Vec<FailedBranch>,
}

/// Plans the prune: resolve the repo, ensure `<onto>` exists, then list every local
/// branch merged into `<onto>` minus `<onto>` itself and the current branch.
pub fn plan(runner: &impl GitRunner, repo: &Path, onto: &str) -> PrunePlan {
    let top = match capture(runner, repo, &["rev-parse", "--show-toplevel"]) {
        Some(top) if !top.is_empty() => top,
        _ => return PrunePlan::Refused("not a git repo".to_string()),
    };
    let top_path = Path::new(&top);

    let current = match capture(runner, top_path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch == "HEAD" => {
            return PrunePlan::Refused("detached HEAD — checkout a branch first".to_string());
        }
        Some(branch) => branch,
        None => return PrunePlan::Refused("not a git repo".to_string()),
    };

    if !onto_exists(runner, top_path, onto) {
        return PrunePlan::Refused(format!("no '{onto}' branch (use --onto <branch>)"));
    }

    let Some(listing) = capture(
        runner,
        top_path,
        &[
            "for-each-ref",
            "--merged",
            onto,
            "--format=%(refname:short) %(objectname:short)",
            "refs/heads/",
        ],
    ) else {
        return PrunePlan::Refused("git for-each-ref failed".to_string());
    };

    let branches: Vec<Branch> = listing
        .lines()
        .filter_map(|line| {
            let (name, sha) = line.trim().split_once(char::is_whitespace)?;
            let name = name.trim();
            let sha = sha.trim();
            if name.is_empty() || name == onto || name == current {
                None
            } else {
                Some(Branch {
                    name: name.to_string(),
                    sha: sha.to_string(),
                })
            }
        })
        .collect();

    if branches.is_empty() {
        return PrunePlan::Nothing(format!("no merged branches to prune (against '{onto}')"));
    }

    PrunePlan::Ready {
        top,
        onto: onto.to_string(),
        branches,
    }
}

/// Deletes each branch with `git branch -D` (safe: ancestry was verified by [`plan`]).
/// Builds a detail block with one `recover:` hint per deletion and a line per failure.
pub fn apply(runner: &impl GitRunner, top: &Path, branches: &[Branch]) -> PruneResult {
    let mut deleted = Vec::new();
    let mut failed = Vec::new();

    for branch in branches {
        match runner.run(top, &["branch", "-D", &branch.name]) {
            Ok(out) if out.exit_code == 0 => deleted.push(branch.clone()),
            Ok(out) => failed.push(FailedBranch {
                name: branch.name.clone(),
                reason: out.error_line(),
            }),
            Err(error) => failed.push(FailedBranch {
                name: branch.name.clone(),
                reason: error.to_string(),
            }),
        }
    }

    let status = match (deleted.len(), failed.len()) {
        (_, 0) => Status::Ok,
        (0, _) => Status::Fail,
        _ => Status::Partial,
    };

    let mut detail = format!("deleted {} branch{}.", deleted.len(), plural(deleted.len()));
    for branch in &deleted {
        let _ = write!(
            detail,
            "\nrecover: git branch {} {}",
            branch.name, branch.sha
        );
    }
    for branch in &failed {
        let _ = write!(detail, "\nfailed: {} — {}", branch.name, branch.reason);
    }

    PruneResult {
        status,
        detail,
        deleted,
        failed,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "es" }
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::{Arc, Mutex},
    };

    use application::ports::GitOutput;

    use super::*;

    #[derive(Clone)]
    struct FakeRunner {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
        results: Arc<Mutex<Vec<GitOutput>>>,
    }
    impl FakeRunner {
        fn new(results: Vec<GitOutput>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                results: Arc::new(Mutex::new(results)),
            }
        }
        fn ok(stdout: &str) -> GitOutput {
            GitOutput {
                stdout: stdout.into(),
                stderr: String::new(),
                exit_code: 0,
            }
        }
        fn exit(stdout: &str, code: i32) -> GitOutput {
            GitOutput {
                stdout: stdout.into(),
                stderr: String::new(),
                exit_code: code,
            }
        }
        fn exit_err(stderr: &str, code: i32) -> GitOutput {
            GitOutput {
                stdout: String::new(),
                stderr: stderr.into(),
                exit_code: code,
            }
        }
        fn arg_lists(&self) -> Vec<Vec<String>> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl GitRunner for FakeRunner {
        fn run(&self, _repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(args.iter().map(std::string::ToString::to_string).collect());
            Ok(self.results.lock().unwrap().remove(0))
        }
    }

    #[test]
    fn plan_ready_lists_merged_branches_minus_onto_and_current() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/cur\n"),
            FakeRunner::ok("refs/heads/main\n"),
            FakeRunner::ok("main deadbee\nfeat/cur 1111111\nfeat/x abc1234\nfix/z def5678\n"),
        ]);
        let plan = plan(&runner, Path::new("."), "main");
        assert_eq!(
            plan,
            PrunePlan::Ready {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                branches: vec![
                    Branch {
                        name: "feat/x".into(),
                        sha: "abc1234".into()
                    },
                    Branch {
                        name: "fix/z".into(),
                        sha: "def5678".into()
                    },
                ],
            }
        );
    }

    #[test]
    fn plan_nothing_when_only_onto_and_current_merged() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/cur\n"),
            FakeRunner::ok("refs/heads/main\n"),
            FakeRunner::ok("main deadbee\nfeat/cur 1111111\n"),
        ]);
        assert_eq!(
            plan(&runner, Path::new("."), "main"),
            PrunePlan::Nothing("no merged branches to prune (against 'main')".into())
        );
    }

    #[test]
    fn plan_refuses_on_detached_head() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("HEAD\n"),
        ]);
        assert_eq!(
            plan(&runner, Path::new("."), "main"),
            PrunePlan::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn plan_refuses_when_onto_missing() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/cur\n"),
            FakeRunner::exit("", 128),
        ]);
        assert_eq!(
            plan(&runner, Path::new("."), "main"),
            PrunePlan::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn plan_refuses_when_not_a_repo() {
        let runner = FakeRunner::new(vec![FakeRunner::exit("", 128)]);
        assert_eq!(
            plan(&runner, Path::new("."), "main"),
            PrunePlan::Refused("not a git repo".into())
        );
    }

    #[test]
    fn apply_deletes_each_branch_and_emits_recovery_hints() {
        let runner = FakeRunner::new(vec![FakeRunner::ok(""), FakeRunner::ok("")]);
        let branches = vec![
            Branch {
                name: "feat/x".into(),
                sha: "abc1234".into(),
            },
            Branch {
                name: "fix/z".into(),
                sha: "def5678".into(),
            },
        ];
        let result = apply(&runner, Path::new("."), &branches);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.deleted.len(), 2);
        assert!(result.failed.is_empty());
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["branch", "-D", "feat/x"],
                vec!["branch", "-D", "fix/z"],
            ]
        );
        assert!(result.detail.contains("recover: git branch feat/x abc1234"));
        assert!(result.detail.contains("recover: git branch fix/z def5678"));
    }

    #[test]
    fn apply_reports_partial_when_one_delete_fails() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""),
            FakeRunner::exit_err("error: branch 'fix/z' is checked out", 1),
        ]);
        let branches = vec![
            Branch {
                name: "feat/x".into(),
                sha: "abc1234".into(),
            },
            Branch {
                name: "fix/z".into(),
                sha: "def5678".into(),
            },
        ];
        let result = apply(&runner, Path::new("."), &branches);
        assert_eq!(result.status, Status::Partial);
        assert_eq!(result.deleted.len(), 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.deleted[0].name, "feat/x");
        assert_eq!(result.failed[0].name, "fix/z");
        assert!(result.detail.contains("fix/z"));
    }

    #[test]
    fn apply_reports_fail_when_all_deletes_fail() {
        let runner = FakeRunner::new(vec![FakeRunner::exit_err("nope", 1)]);
        let branches = vec![Branch {
            name: "feat/x".into(),
            sha: "abc1234".into(),
        }];
        let result = apply(&runner, Path::new("."), &branches);
        assert_eq!(result.status, Status::Fail);
        assert!(result.deleted.is_empty());
        assert_eq!(result.failed.len(), 1);
    }
}
