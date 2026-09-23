use std::time::Duration;

use gtl_application::viewer::push::{PushError, PushPlan, PushRepository, ViewerPushGit};
use gtl_models::{
    diffs::CommitId,
    git::{BranchName, GitRefName, RemoteName, RemoteUrl},
    paths::RepositoryRoot,
};

use super::HybridGitClient;
use crate::git_process::{self, GitProcessOutput};

fn run(path: &RepositoryRoot, args: &[&str]) -> Result<GitProcessOutput, PushError> {
    git_process::run_bounded(path.as_ref(), args, Duration::from_secs(10))
        .map_err(|error| PushError::Git(error.to_string()))
}

fn read(path: &RepositoryRoot, args: &[&str]) -> Result<String, PushError> {
    let output = run(path, args)?;
    if !output.success() {
        return Err(PushError::Git(
            output.diagnostic().chars().take(4096).collect(),
        ));
    }
    Ok(output.stdout.trim().to_owned())
}

impl ViewerPushGit for HybridGitClient {
    fn inspect_push(&self, path: &RepositoryRoot) -> Result<PushRepository, PushError> {
        let branch = run(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        if !branch.success() {
            return Err(PushError::Detached);
        }
        let branch = BranchName::try_new(branch.stdout.trim()).map_err(|_| PushError::Detached)?;
        let local_ref = format!("refs/heads/{branch}");
        let data = read(
            path,
            &[
                "for-each-ref",
                "--format=%(upstream:remotename)%00%(upstream:remoteref)%00%(upstream)",
                "--",
                &local_ref,
            ],
        )?;
        let (remote, rest) = data.split_once('\0').ok_or(PushError::NoUpstream)?;
        let (destination, upstream_ref) = rest.split_once('\0').ok_or(PushError::NoUpstream)?;
        if remote.is_empty() || remote == "." || !destination.starts_with("refs/heads/") {
            return Err(PushError::NoUpstream);
        }
        let remote = RemoteName::try_new(remote).map_err(|_| PushError::NoUpstream)?;
        let destination = GitRefName::try_new(destination).map_err(|_| PushError::NoUpstream)?;
        let urls = read(
            path,
            &[
                "remote",
                "get-url",
                "--push",
                "--all",
                "--",
                remote.as_ref(),
            ],
        )?;
        let mut urls = urls.lines();
        let url = urls.next().ok_or(PushError::NoUpstream)?;
        if urls.next().is_some() {
            return Err(PushError::MultipleDestinations);
        }
        let url = RemoteUrl::try_new(url).map_err(|_| PushError::NoUpstream)?;
        let head = read(
            path,
            &["rev-parse", "--verify", &format!("{local_ref}^{{commit}}")],
        )?
        .parse()
        .map_err(|_| PushError::NothingToPush)?;
        let upstream = read(
            path,
            &[
                "rev-parse",
                "--verify",
                &format!("{upstream_ref}^{{commit}}"),
            ],
        )?
        .parse()
        .map_err(|_| PushError::NoUpstream)?;
        Ok(PushRepository {
            branch,
            remote,
            destination,
            url,
            head,
            upstream,
        })
    }

    fn contains_commit(
        &self,
        path: &RepositoryRoot,
        commit: &CommitId,
        head: &CommitId,
    ) -> Result<bool, PushError> {
        let output = run(
            path,
            &[
                "merge-base",
                "--is-ancestor",
                commit.as_ref(),
                head.as_ref(),
            ],
        )?;
        // A missing object and a rewritten branch both invalidate the review.
        Ok(output.success())
    }

    fn count_commits(
        &self,
        path: &RepositoryRoot,
        base: &CommitId,
        head: &CommitId,
    ) -> Result<u64, PushError> {
        read(
            path,
            &["rev-list", "--count", &format!("{base}..{head}"), "--"],
        )?
        .parse()
        .map_err(|_| PushError::Git("Invalid commit count".into()))
    }

    fn push_commit(&self, plan: &PushPlan) -> Result<(), PushError> {
        let arguments = plan.arguments();
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        let output =
            git_process::run_bounded(plan.path().as_ref(), &arguments, Duration::from_mins(2))
                .map_err(|error| PushError::Git(error.to_string()))?;
        if output.success() {
            Ok(())
        } else {
            Err(PushError::Git(
                output.diagnostic().chars().take(4096).collect(),
            ))
        }
    }
}
