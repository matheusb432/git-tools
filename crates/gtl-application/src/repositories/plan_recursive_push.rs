use std::path::PathBuf;

use gtl_models::{
    failure::ErrorMeta,
    git::{CommitCount, GitHead, GitRange},
    paths::{ProjectName, RepositoryRoot},
    repository::{
        recursive_push::{Dest, RepoTarget, SubreposPlan},
        traversal::RepositoryTraversalScope,
    },
};

use crate::{ports::GitClient, repositories::find_repositories};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum PlanRecursivePushError {
    #[error(transparent)]
    #[meta(transparent)]
    Discover(#[from] find_repositories::FindRepositoriesError),
}

#[cqrsy::query]
pub fn execute(
    root: PathBuf,
    git: &impl GitClient,
) -> Result<SubreposPlan, PlanRecursivePushError> {
    let repositories_empty_detail = format!("no git repos found under {}", root.display());
    let discovered = find_repositories::execute(find_repositories::FindRepositories {
        root,
        scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
    })?;
    if discovered.is_empty() {
        return Ok(SubreposPlan::Refused(repositories_empty_detail));
    }
    let targets = discovered
        .into_iter()
        .map(|repo| inspect(git, &repo.path, repo.label))
        .collect();
    Ok(SubreposPlan::Ready(targets))
}

fn inspect(git: &impl GitClient, path: &RepositoryRoot, label: ProjectName) -> RepoTarget {
    let dest = match git.current_branch(path).ok() {
        Some(GitHead::Branch(branch)) => match git.branch_remote(path, &branch).ok().flatten() {
            Some(remote) if is_synced(git, path) => Dest::Synced { branch, remote },
            Some(remote) => Dest::Push { branch, remote },
            None => Dest::Skip {
                reason: "no upstream tracking branch".to_string(),
            },
        },
        _ => Dest::Skip {
            reason: "detached HEAD".to_string(),
        },
    };
    RepoTarget {
        path: path.clone(),
        label,
        dest,
    }
}

fn is_synced(git: &impl GitClient, path: &RepositoryRoot) -> bool {
    git.commit_count(path, &GitRange::upstream_to_head())
        .ok()
        .flatten()
        .is_some_and(|count| count == CommitCount::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        repositories::plan_recursive_push,
        utils::{self, ScriptedGitClient},
    };

    #[test]
    fn inspect_resolves_push_when_branch_is_ahead() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin\n"),
            ScriptedGitClient::applied("2\n"),
        ]);
        let target = inspect(
            &runner,
            &utils::repository_root("//fixture.invalid/repositories/repos/api"),
            utils::project_name("api"),
        );
        assert_eq!(
            target.dest,
            Dest::Push {
                branch: crate::utils::branch_name("main"),
                remote: crate::utils::remote_name("origin"),
            }
        );
    }

    #[test]
    fn inspect_marks_synced_when_no_unpushed_commits() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin\n"),
            ScriptedGitClient::applied("0\n"),
        ]);
        let target = inspect(
            &runner,
            &utils::repository_root("//fixture.invalid/repositories/repos/api"),
            utils::project_name("api"),
        );
        assert_eq!(
            target.dest,
            Dest::Synced {
                branch: crate::utils::branch_name("main"),
                remote: crate::utils::remote_name("origin"),
            }
        );
    }

    #[test]
    fn inspect_pushes_when_ahead_count_is_unavailable() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin\n"),
            ScriptedGitClient::rejected(""),
        ]);
        let target = inspect(
            &runner,
            &utils::repository_root("//fixture.invalid/repositories/repos/api"),
            utils::project_name("api"),
        );
        assert_eq!(
            target.dest,
            Dest::Push {
                branch: crate::utils::branch_name("main"),
                remote: crate::utils::remote_name("origin"),
            }
        );
    }

    #[test]
    fn inspect_skips_branch_without_upstream() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("feat\n"),
            ScriptedGitClient::rejected(""),
        ]);
        let target = inspect(
            &runner,
            &utils::repository_root("//fixture.invalid/repositories/repos/api"),
            utils::project_name("api"),
        );
        assert_eq!(
            target.dest,
            Dest::Skip {
                reason: "no upstream tracking branch".into(),
            }
        );
    }

    #[test]
    fn inspect_skips_detached_head() {
        let runner = ScriptedGitClient::new(vec![ScriptedGitClient::applied("HEAD\n")]);
        let target = inspect(
            &runner,
            &utils::repository_root("//fixture.invalid/repositories/repos/api"),
            utils::project_name("api"),
        );
        assert_eq!(
            target.dest,
            Dest::Skip {
                reason: "detached HEAD".into(),
            }
        );
    }

    #[test]
    fn plan_refuses_when_no_repos_are_discovered() {
        let temporary = tempfile::tempdir().unwrap();
        let plan = plan_recursive_push::execute(
            temporary.path().to_path_buf(),
            &ScriptedGitClient::default(),
        )
        .unwrap();

        assert_eq!(
            plan,
            SubreposPlan::Refused(format!(
                "no git repos found under {}",
                temporary.path().display()
            ))
        );
    }

    #[test]
    fn plan_inspects_every_discovered_repo_with_its_label() {
        let temporary = tempfile::tempdir().unwrap();
        utils::make_repository(&temporary.path().join("api"));
        utils::make_repository(&temporary.path().join("libs/inner"));
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("HEAD\n"),
            ScriptedGitClient::applied("HEAD\n"),
        ]);

        let plan = plan_recursive_push::execute(temporary.path().to_path_buf(), &runner).unwrap();

        let targets = match plan {
            SubreposPlan::Ready(targets) => Some(targets),
            SubreposPlan::Refused(_) => None,
        }
        .unwrap();
        let labels: Vec<&str> = targets.iter().map(|target| target.label.as_str()).collect();
        assert_eq!(labels, ["api", "libs/inner"]);
    }
}
