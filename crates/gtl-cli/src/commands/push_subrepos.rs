use std::{fmt::Write as _, path::Path};

use anyhow::Context as _;
use gtl_models::{
    git::{BranchName, RemoteName},
    paths::{ProjectName, RepositoryRoot},
    repository::recursive_push::{
        Dest, PushAllResult, RepoOutcome, RepoReport, RepoTarget, Status,
    },
};
use gtl_wire::v1;

pub(crate) fn targets_from_grpc(
    targets: Vec<v1::RecursivePushTarget>,
) -> anyhow::Result<Vec<RepoTarget>> {
    targets.into_iter().map(target_from_grpc).collect()
}

pub(crate) fn targets_to_grpc(targets: &[RepoTarget]) -> Vec<v1::RecursivePushTarget> {
    targets.iter().map(target_to_grpc).collect()
}

pub(crate) fn result_from_grpc(
    response: v1::ExecuteRecursiveRepositoryPushResponse,
) -> anyhow::Result<PushAllResult> {
    let status = match v1::RecursivePushStatus::try_from(response.status) {
        Ok(v1::RecursivePushStatus::Ok) => Status::Ok,
        Ok(v1::RecursivePushStatus::Partial) => Status::Partial,
        Ok(v1::RecursivePushStatus::Failed) => Status::Fail,
        Ok(v1::RecursivePushStatus::Unspecified) | Err(_) => {
            anyhow::bail!("gtl-server returned an invalid recursive-push status")
        }
    };
    let reports = response
        .results
        .into_iter()
        .map(|result| {
            let detail = result.detail.unwrap_or_default();
            let outcome = match v1::RecursivePushRepositoryStatus::try_from(result.status) {
                Ok(v1::RecursivePushRepositoryStatus::Pushed) => RepoOutcome::Pushed,
                Ok(v1::RecursivePushRepositoryStatus::UpToDate) => RepoOutcome::UpToDate,
                Ok(v1::RecursivePushRepositoryStatus::Skipped) => RepoOutcome::Skipped(detail),
                Ok(v1::RecursivePushRepositoryStatus::Failed) => RepoOutcome::Failed(detail),
                Ok(v1::RecursivePushRepositoryStatus::Unspecified) | Err(_) => {
                    anyhow::bail!("gtl-server returned an invalid recursive-push repository status")
                }
            };
            Ok(RepoReport {
                label: ProjectName::try_new(result.label)
                    .context("gtl-server returned an empty recursive-push label")?,
                outcome,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PushAllResult { status, reports })
}

fn target_from_grpc(target: v1::RecursivePushTarget) -> anyhow::Result<RepoTarget> {
    let destination = target
        .destination
        .context("gtl-server returned no recursive-push destination")?;
    let dest = match destination {
        v1::recursive_push_target::Destination::Push(destination) => {
            destination_from_grpc(destination, true)?
        }
        v1::recursive_push_target::Destination::Synced(destination) => {
            destination_from_grpc(destination, false)?
        }
        v1::recursive_push_target::Destination::Skip(reason) => Dest::Skip {
            reason: reason.detail,
        },
    };
    Ok(RepoTarget {
        path: RepositoryRoot::try_new(target.repository_root.into())
            .context("gtl-server returned a non-absolute recursive-push root")?,
        label: ProjectName::try_new(target.label)
            .context("gtl-server returned an empty recursive-push label")?,
        dest,
    })
}

fn destination_from_grpc(destination: v1::PushDestination, push: bool) -> anyhow::Result<Dest> {
    let branch = BranchName::try_new(destination.branch)
        .context("gtl-server returned an empty recursive-push branch")?;
    let remote = RemoteName::try_new(destination.remote)
        .context("gtl-server returned an empty recursive-push remote")?;
    Ok(if push {
        Dest::Push { branch, remote }
    } else {
        Dest::Synced { branch, remote }
    })
}

fn target_to_grpc(target: &RepoTarget) -> v1::RecursivePushTarget {
    let destination = match &target.dest {
        Dest::Push { branch, remote } => {
            v1::recursive_push_target::Destination::Push(destination_to_grpc(branch, remote))
        }
        Dest::Synced { branch, remote } => {
            v1::recursive_push_target::Destination::Synced(destination_to_grpc(branch, remote))
        }
        Dest::Skip { reason } => {
            v1::recursive_push_target::Destination::Skip(v1::OperationDetail {
                detail: reason.clone(),
            })
        }
    };
    v1::RecursivePushTarget {
        repository_root: target.path.to_string(),
        label: target.label.to_string(),
        destination: Some(destination),
    }
}

fn destination_to_grpc(branch: &BranchName, remote: &RemoteName) -> v1::PushDestination {
    v1::PushDestination {
        branch: branch.to_string(),
        remote: remote.to_string(),
    }
}

pub(crate) fn confirmation(root: &Path, targets: &[RepoTarget]) -> String {
    let mut out = format!(
        "push -r: push {} repo(s) under {}:",
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
                let _ = write!(out, "\n  {}  (skip: {reason})", target.label);
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
            text.contains("web  (skip: no upstream tracking branch)"),
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
