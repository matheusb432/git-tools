//! Prunes merged branches across already-resolved managed repositories.

use gtl_models::managed::ManagedRepo;

use crate::{
    branches::{
        apply_prune::{self, ApplyPrune, ApplyPruneError, ApplyPruneOk},
        plan_prune::{self, PlanPrune, PlanPruneError, PlanPruneOk, PruneBranch},
    },
    ports::GitClient,
};

/// Requests one branch-prune attempt for every resolved managed repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneAll {
    pub repos: Vec<ManagedRepo>,
    pub onto: String,
    pub dry: bool,
}

/// Describes the complete prune decision for one managed repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruneAction {
    /// The repository is not present on this machine.
    Absent,
    /// Repository state refused pruning.
    Refused(String),
    /// No merged branches require deletion.
    Nothing(String),
    /// A dry run found branches that a real run would delete.
    WouldDelete(Vec<PruneBranch>),
    /// Git attempted every planned deletion and returned their structured outcomes.
    Applied(ApplyPruneOk),
}

/// Reports one managed repository's prune decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneRepoResult {
    pub name: String,
    pub action: PruneAction,
}

/// Classifies the aggregate managed prune result for CLI exit mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneExit {
    Clean,
    Warn,
}

/// Reports every repository result and the aggregate exit classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneAllOk {
    pub exit: PruneExit,
    pub results: Vec<PruneRepoResult>,
}

/// Reports an unexpected Git transport failure while pruning managed repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PruneAllError {
    /// Git transport failed while processing one repository.
    #[error("managed prune failed for '{failed_repo}': {source}")]
    Transport {
        /// Identifies the repository whose Git invocation failed.
        failed_repo: String,
        /// Preserves every repository result completed before the failure.
        completed_results: Vec<PruneRepoResult>,
        /// Preserves the failed repository's completed deletions, when any exist.
        failed_result: Option<Box<PruneRepoResult>>,
        /// Preserves the original Git transport failure.
        #[source]
        source: anyhow::Error,
    },
}

#[derive(Debug)]
struct PruneAttemptError {
    failed_action: Option<PruneAction>,
    source: anyhow::Error,
}

/// Plans and optionally applies branch pruning across managed repositories.
///
/// # Errors
///
/// Returns [`PruneAllError`] when Git cannot be executed. Results completed before the transport
/// failure remain available on the error.
#[cqrsy::command]
pub fn execute(command: PruneAll, git: &impl GitClient) -> Result<PruneAllOk, PruneAllError> {
    let PruneAll { repos, onto, dry } = command;
    let mut results = Vec::with_capacity(repos.len());
    for repo in repos {
        match prune_one(&repo.path, &onto, dry, git) {
            Ok(action) => results.push(PruneRepoResult {
                name: repo.name,
                action,
            }),
            Err(error) => {
                return Err(PruneAllError::Transport {
                    failed_repo: repo.name.clone(),
                    completed_results: results,
                    failed_result: error.failed_action.map(|action| {
                        Box::new(PruneRepoResult {
                            name: repo.name,
                            action,
                        })
                    }),
                    source: error.source,
                });
            }
        }
    }
    let exit = if results.iter().any(|result| {
        matches!(&result.action, PruneAction::Applied(applied) if !applied.failed.is_empty())
    }) {
        PruneExit::Warn
    } else {
        PruneExit::Clean
    };

    Ok(PruneAllOk { exit, results })
}

fn prune_one(
    repo_path: &std::path::Path,
    onto: &str,
    dry: bool,
    git: &impl GitClient,
) -> Result<PruneAction, PruneAttemptError> {
    if !git.repo_present(repo_path) {
        return Ok(PruneAction::Absent);
    }

    let plan = plan_prune::execute(
        PlanPrune {
            repo_path: repo_path.to_path_buf(),
            onto: onto.into(),
        },
        git,
    )
    .map_err(|error| match error {
        PlanPruneError::Transport { source } => PruneAttemptError {
            failed_action: None,
            source,
        },
    })?;
    match plan {
        PlanPruneOk::Refused(detail) => Ok(PruneAction::Refused(detail)),
        PlanPruneOk::Nothing(detail) => Ok(PruneAction::Nothing(detail)),
        PlanPruneOk::Ready { branches, .. } if dry => Ok(PruneAction::WouldDelete(branches)),
        PlanPruneOk::Ready { top, branches, .. } => {
            apply_prune::execute(ApplyPrune { top, branches }, git)
                .map(PruneAction::Applied)
                .map_err(|error| match error {
                    ApplyPruneError::Transport {
                        completed_result,
                        source,
                    } => PruneAttemptError {
                        failed_action: completed_result.map(|result| PruneAction::Applied(*result)),
                        source,
                    },
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::managed::ManagedRepo;

    use super::{PruneAction, PruneAll, PruneAllError, PruneExit, execute};
    use crate::{
        branches::{
            apply_prune::{PruneFailure, PruneStatus},
            plan_prune::PruneBranch,
        },
        testing::ScriptedGitClient,
    };

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.into(),
            path: PathBuf::from(format!("/repos/{name}")),
            remote: String::new(),
        }
    }

    fn prune(repos: Vec<ManagedRepo>, dry: bool) -> PruneAll {
        PruneAll {
            repos,
            onto: "main".into(),
            dry,
        }
    }

    #[test]
    fn absent_repository_is_a_closed_clean_result() {
        let git = ScriptedGitClient::default();
        git.absent_repos.lock().unwrap().push("/repos/api".into());

        let result = execute(prune(vec![repo("api")], false), &git)
            .expect("an absent repository remains a closed result");

        assert_eq!(result.exit, PruneExit::Clean);
        assert_eq!(result.results[0].name, "api");
        assert_eq!(result.results[0].action, PruneAction::Absent);
    }

    #[test]
    fn dry_run_returns_the_branch_plan_without_deleting() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied("main aaaaaaa\nfeature/done bbbbbbb\n"),
        ]);

        let result = execute(prune(vec![repo("api")], true), &git)
            .expect("a dry-run plan remains a closed result");

        assert_eq!(result.exit, PruneExit::Clean);
        assert_eq!(
            result.results[0].action,
            PruneAction::WouldDelete(vec![PruneBranch {
                name: "feature/done".into(),
                id: crate::testing::commit_id_fixture("bbbbbbb"),
            }])
        );
    }

    #[test]
    fn later_plan_transport_failure_preserves_earlier_repository_deletions() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("/repos/api\n")),
            Ok(ScriptedGitClient::applied("main\n")),
            Ok(ScriptedGitClient::applied("refs/heads/main\n")),
            Ok(ScriptedGitClient::applied(
                "main aaaaaaa\nfeature/api bbbbbbb\n",
            )),
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(prune(vec![repo("api"), repo("web")], false), &git)
            .expect_err("the second repository planning transport must fail");

        let PruneAllError::Transport {
            failed_repo,
            completed_results,
            failed_result,
            source,
        } = error;
        assert_eq!(failed_repo, "web");
        assert_eq!(completed_results.len(), 1);
        let PruneAction::Applied(api) = &completed_results[0].action else {
            panic!("the first repository must retain its applied result");
        };
        assert_eq!(api.status, PruneStatus::Ok);
        assert_eq!(
            api.deleted,
            vec![PruneBranch {
                name: "feature/api".into(),
                id: crate::testing::commit_id_fixture("bbbbbbb"),
            }]
        );

        assert_eq!(failed_result, None);
        assert_eq!(source.to_string(), "git transport unavailable");
    }

    #[test]
    fn apply_transport_preserves_partial_current_repository_deletions() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("/repos/api\n")),
            Ok(ScriptedGitClient::applied("main\n")),
            Ok(ScriptedGitClient::applied("refs/heads/main\n")),
            Ok(ScriptedGitClient::applied(
                "main aaaaaaa\nfeature/first bbbbbbb\nfeature/second ccccccc\n",
            )),
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(prune(vec![repo("api")], false), &git)
            .expect_err("the second deletion transport must fail");

        let PruneAllError::Transport {
            failed_repo,
            completed_results,
            failed_result,
            source,
        } = error;
        assert_eq!(failed_repo, "api");
        assert_eq!(completed_results, Vec::new());
        let failed_result = failed_result.expect("the completed deletion must be preserved");
        let PruneAction::Applied(applied) = failed_result.action else {
            panic!("the failed repository must retain its partial applied result");
        };
        assert_eq!(applied.status, PruneStatus::Ok);
        assert_eq!(
            applied.deleted,
            vec![PruneBranch {
                name: "feature/first".into(),
                id: crate::testing::commit_id_fixture("bbbbbbb"),
            }]
        );
        assert_eq!(applied.failed, Vec::<PruneFailure>::new());
        assert_eq!(source.to_string(), "git transport unavailable");
    }
}
