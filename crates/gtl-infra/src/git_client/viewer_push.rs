use std::time::Duration;

use gtl_application::viewer::push::{PushError, PushPlan, PushRepository, ViewerPushGit};
use gtl_models::{
    diffs::CommitId,
    failure::{ExternalDiagnostic, PushFailure, PushRefRejection, RejectedPushRef},
    git::{BranchName, GitRefName, RemoteName, RemoteUrl},
    paths::RepositoryRoot,
};

use super::HybridGitClient;
use crate::git_process::{self, GitProcessOutput};

fn git_failed(diagnostic: &str) -> PushError {
    PushFailure::GitFailed {
        diagnostic: ExternalDiagnostic::new(diagnostic),
    }
    .into()
}

fn run(path: &RepositoryRoot, args: &[&str]) -> Result<GitProcessOutput, PushError> {
    git_process::run_bounded(path.as_ref(), args, Duration::from_secs(10))
        .map_err(|error| git_failed(&format!("{error:#}")))
}

fn read(path: &RepositoryRoot, args: &[&str]) -> Result<String, PushError> {
    let output = run(path, args)?;
    if !output.success() {
        return Err(git_failed(output.diagnostic()));
    }
    Ok(output.stdout.trim().to_owned())
}

impl ViewerPushGit for HybridGitClient {
    fn inspect_push(&self, path: &RepositoryRoot) -> Result<PushRepository, PushError> {
        let branch = run(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        if !branch.success() {
            return Err(PushFailure::Detached.into());
        }
        let branch =
            BranchName::try_new(branch.stdout.trim()).map_err(|_| PushFailure::Detached)?;
        let no_upstream = || PushFailure::NoUpstream {
            branch: branch.clone(),
        };
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
        let (remote, rest) = data.split_once('\0').ok_or_else(no_upstream)?;
        let (destination, upstream_ref) = rest.split_once('\0').ok_or_else(no_upstream)?;
        if remote.is_empty() || remote == "." || !destination.starts_with("refs/heads/") {
            return Err(no_upstream().into());
        }
        let remote = RemoteName::try_new(remote).map_err(|_| no_upstream())?;
        let destination_branch = BranchName::try_new(
            destination
                .strip_prefix("refs/heads/")
                .ok_or_else(no_upstream)?,
        )
        .map_err(|_| no_upstream())?;
        let destination = GitRefName::try_new(destination).map_err(|_| no_upstream())?;
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
        let url = urls.next().ok_or_else(no_upstream)?;
        if urls.next().is_some() {
            return Err(PushFailure::MultipleDestinations { remote }.into());
        }
        let url = RemoteUrl::try_new(url).map_err(|_| no_upstream())?;
        let head = read(
            path,
            &["rev-parse", "--verify", &format!("{local_ref}^{{commit}}")],
        )?
        .parse()
        .map_err(|_| PushFailure::NothingToPush)?;
        let upstream = read(
            path,
            &[
                "rev-parse",
                "--verify",
                &format!("{upstream_ref}^{{commit}}"),
            ],
        )?
        .parse()
        .map_err(|_| no_upstream())?;
        Ok(PushRepository {
            branch,
            remote,
            destination,
            destination_branch,
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
        let count = read(
            path,
            &["rev-list", "--count", &format!("{base}..{head}"), "--"],
        )?;
        count
            .parse()
            .map_err(|_| git_failed(&format!("git rev-list printed an invalid count: {count}")))
    }

    fn push_commit(&self, plan: &PushPlan) -> Result<(), PushError> {
        let arguments = plan.arguments();
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        let output =
            git_process::run_bounded(plan.path().as_ref(), &arguments, Duration::from_mins(2))
                .map_err(|error| git_failed(&format!("{error:#}")))?;
        if output.success() {
            return Ok(());
        }
        let refs = rejected_refs(&output.stdout);
        let diagnostic = ExternalDiagnostic::new(output.diagnostic());
        Err(if refs.is_empty() {
            PushFailure::GitFailed { diagnostic }
        } else {
            PushFailure::Rejected { refs, diagnostic }
        }
        .into())
    }
}

/// Reads refused refs from `git push --porcelain` lines: `<flag>\t<from>:<to>\t<summary>`.
fn rejected_refs(porcelain: &str) -> Vec<RejectedPushRef> {
    porcelain
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let (flag, refs, summary) = (fields.next()?, fields.next()?, fields.next()?);
            if flag != "!" {
                return None;
            }
            let destination = refs.rsplit_once(':').map_or(refs, |(_, to)| to);
            Some(RejectedPushRef {
                destination: GitRefName::try_new(destination).ok()?,
                reason: ref_rejection(summary.trim()),
            })
        })
        .collect()
}

fn ref_rejection(summary: &str) -> PushRefRejection {
    let reason = summary
        .split_once(" (")
        .map(|(_, reason)| reason.trim_end_matches(')'));
    if summary.starts_with("[rejected]") {
        match reason {
            Some("fetch first") => PushRefRejection::FetchFirst,
            Some("non-fast-forward") => PushRefRejection::NonFastForward,
            _ => PushRefRejection::Other {
                summary: ExternalDiagnostic::new(summary),
            },
        }
    } else if summary.starts_with("[remote rejected]") {
        PushRefRejection::RemoteRejected {
            message: ExternalDiagnostic::new(reason.unwrap_or(summary)),
        }
    } else {
        PushRefRejection::Other {
            summary: ExternalDiagnostic::new(summary),
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{ExternalDiagnostic, PushRefRejection, RejectedPushRef};

    use super::rejected_refs;

    fn rejected(destination: &str, reason: PushRefRejection) -> RejectedPushRef {
        RejectedPushRef {
            destination: destination.try_into().unwrap(),
            reason,
        }
    }

    #[test]
    fn porcelain_rejections_are_classified_per_ref() {
        let porcelain = "To //fixture.invalid/repositories/remote.git\n\
            !\t0123456789abcdef0123456789abcdef01234567:refs/heads/main\t[rejected] (fetch first)\n\
            !\trefs/heads/topic:refs/heads/topic\t[rejected] (non-fast-forward)\n\
            !\tHEAD:refs/heads/release\t[remote rejected] (protected branch hook declined)\n\
            !\tHEAD:refs/heads/odd\t[rejected] (stale info)\n\
            =\trefs/heads/done:refs/heads/done\t[up to date]\n\
            Done\n";

        assert_eq!(
            rejected_refs(porcelain),
            [
                rejected("refs/heads/main", PushRefRejection::FetchFirst),
                rejected("refs/heads/topic", PushRefRejection::NonFastForward),
                rejected(
                    "refs/heads/release",
                    PushRefRejection::RemoteRejected {
                        message: ExternalDiagnostic::new("protected branch hook declined"),
                    },
                ),
                rejected(
                    "refs/heads/odd",
                    PushRefRejection::Other {
                        summary: ExternalDiagnostic::new("[rejected] (stale info)"),
                    },
                ),
            ]
        );
    }

    #[test]
    fn output_without_rejected_refs_has_no_rejections() {
        assert_eq!(
            rejected_refs("To //fixture.invalid/repositories/remote.git\nDone\n"),
            Vec::<gtl_models::failure::RejectedPushRef>::new()
        );
        assert_eq!(
            rejected_refs(""),
            Vec::<gtl_models::failure::RejectedPushRef>::new()
        );
    }
}
