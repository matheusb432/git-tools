//! Publishes pending local tags and advances their origin tracking refs.

use std::path::{Path, PathBuf};

use domain::tags::Tag;

use super::{
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagActionStatus, TagOperationProgress},
    parse,
};
use crate::ports::GitRunner;

/// Requests publication of every local tag missing from the origin tracking refs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTags {
    pub repo: PathBuf,
}

/// Reports an unexpected Git transport failure while publishing tags.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PushTagsError {
    /// Git could not be started or its output could not be collected.
    #[error("{source}")]
    Unexpected {
        progress: TagOperationProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Publishes pending local tags with explicit source and destination refspecs.
///
/// # Errors
///
/// Returns [`PushTagsError`] when Git cannot be executed.
#[cqrsy::command]
pub fn execute(command: PushTags, git: &impl GitRunner) -> Result<TagActionOutcome, PushTagsError> {
    match push(command, git) {
        Ok(outcome) => Ok(outcome),
        Err(GitCommandError::Rejected { detail, progress }) => {
            Ok(TagActionOutcome::failed(detail).with_progress(*progress))
        }
        Err(GitCommandError::Transport { source, progress }) => Err(PushTagsError::Unexpected {
            progress: *progress,
            source,
        }),
    }
}

fn push(command: PushTags, git: &impl GitRunner) -> Result<TagActionOutcome, GitCommandError> {
    let PushTags { repo } = command;
    let refs = parse::load(git, &repo)?;
    let pending = refs.pending().into_iter().cloned().collect::<Vec<_>>();
    if pending.is_empty() {
        return Ok(TagActionOutcome::new(
            TagActionStatus::Noop,
            "tags already up to date",
        ));
    }

    push_tags(git, &repo, &pending, TagOperationProgress::default())
}

pub(super) fn push_named(
    git: &impl GitRunner,
    repo: &Path,
    names: &[String],
    created: TagActionOutcome,
) -> Result<TagActionOutcome, GitCommandError> {
    let refs = parse::load(git, repo)
        .map_err(|error| error.with_prior_progress(created.progress.clone()))?;

    let mut pending = Vec::new();
    for name in names {
        let Some(tag) = refs.local(name) else {
            return Ok(
                TagActionOutcome::failed(format!("created tag {name} was not found"))
                    .with_progress(created.progress),
            );
        };
        if !refs.is_remote(tag) {
            pending.push(tag.clone());
        }
    }

    if pending.is_empty() {
        return Ok(TagActionOutcome::new(TagActionStatus::Noop, created.detail)
            .with_progress(created.progress));
    }

    let created_detail = created.detail;
    Ok(push_tags(git, repo, &pending, created.progress)?.with_created_detail(created_detail))
}

fn push_tags(
    git: &impl GitRunner,
    repo: &Path,
    pending: &[Tag],
    mut progress: TagOperationProgress,
) -> Result<TagActionOutcome, GitCommandError> {
    let mut args = vec!["push".to_string(), "origin".to_string()];
    args.extend(push_refspecs(pending));
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();

    let names = pending
        .iter()
        .map(|tag| tag.name().to_string())
        .collect::<Vec<_>>();
    progress.record_push_attempt(&names);
    let output = git
        .run(repo, &args)
        .map_err(GitCommandError::from)
        .map_err(|error| error.with_prior_progress(progress.clone()))?;
    if output.exit_code != 0 {
        return Err(
            GitCommandError::rejected(output.fail_detail("git push tags failed"))
                .with_prior_progress(progress),
        );
    }

    progress.record_push_completed(&names);

    for tag in pending {
        track(git, repo, tag).map_err(|error| error.with_prior_progress(progress.clone()))?;
        progress.record_tracking_updated(tag.name());
    }
    let names = pending.iter().map(Tag::name).collect::<Vec<_>>().join(", ");
    let noun = if pending.len() == 1 { "tag" } else { "tags" };
    Ok(TagActionOutcome::new(
        TagActionStatus::Pushed,
        format!("pushed {} {noun}: {names}", pending.len()),
    )
    .with_progress(progress))
}

fn push_refspecs(tags: &[Tag]) -> Vec<String> {
    tags.iter()
        .map(|tag| format!("refs/tags/{0}:refs/tags/{0}", tag.name()))
        .collect()
}

fn track(git: &impl GitRunner, repo: &Path, tag: &Tag) -> Result<(), GitCommandError> {
    let remote_ref = format!("refs/remotes/origin/tags/{}", tag.name());
    let output = git.run(repo, &["update-ref", &remote_ref, tag.object()])?;
    if output.exit_code == 0 {
        Ok(())
    } else {
        Err(GitCommandError::rejected(output.fail_detail(&format!(
            "git update-ref failed for {}",
            tag.name()
        ))))
    }
}

#[cfg(test)]
mod tests {
    use domain::tags::Tag;

    use super::{PushTags, PushTagsError, execute, push_refspecs};
    use crate::{
        tags::outcome::{
            TagActionOutcome, TagActionStatus, TagOperationProgress, TagRemotePushProgress,
        },
        testing::FakeGitRunner,
    };

    const LOCAL_TAG: &str = "object-v1\t\t\tv1.0.0\t\t100\n";

    fn tag(name: &str) -> Tag {
        let commit = format!("object-{name}");
        Tag::lightweight(name.into(), commit, format!("short-{name}"), None)
    }

    #[test]
    fn push_refspecs_name_each_tag_source_and_destination_explicitly() {
        assert_eq!(
            push_refspecs(&[tag("v1.0.0"), tag("stable")]),
            vec![
                "refs/tags/v1.0.0:refs/tags/v1.0.0",
                "refs/tags/stable:refs/tags/stable",
            ]
        );
    }

    #[test]
    fn load_transport_failure_remains_an_error_with_its_source() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(PushTags { repo: ".".into() }, &git)
            .expect_err("transport failure must remain an error");

        assert_transport_error(&error);
    }

    #[test]
    fn load_nonzero_exit_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::exit_err(
            "fatal: refs unavailable",
            128,
        )]);

        assert_eq!(
            execute(PushTags { repo: ".".into() }, &git)
                .expect("a Git rejection is a closed action failure"),
            failed("git for-each-ref failed: fatal: refs unavailable")
        );
    }

    #[test]
    fn push_transport_failure_remains_an_error_with_its_source() {
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok(LOCAL_TAG)),
            Ok(FakeGitRunner::ok("")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(PushTags { repo: ".".into() }, &git)
            .expect_err("transport failure must remain an error");

        let PushTagsError::Unexpected { progress, source } = error;
        assert_eq!(
            progress.remote_push,
            TagRemotePushProgress::Indeterminate {
                attempted_refs: vec!["refs/tags/v1.0.0".into()],
            }
        );
        assert_eq!(source.to_string(), "git transport unavailable");
    }

    #[test]
    fn push_nonzero_exit_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(LOCAL_TAG),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("fatal: remote rejected", 1),
        ]);

        let outcome = execute(PushTags { repo: ".".into() }, &git)
            .expect("a Git rejection is a closed action failure");

        assert_eq!(
            outcome.detail,
            "git push tags failed: fatal: remote rejected"
        );
        assert_eq!(
            outcome.progress.remote_push,
            TagRemotePushProgress::Indeterminate {
                attempted_refs: vec!["refs/tags/v1.0.0".into()],
            }
        );
    }

    #[test]
    fn tracking_transport_failure_remains_an_error_with_its_source() {
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok(LOCAL_TAG)),
            Ok(FakeGitRunner::ok("")),
            Ok(FakeGitRunner::ok("")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(PushTags { repo: ".".into() }, &git)
            .expect_err("transport failure must remain an error");

        assert_transport_error(&error);
    }

    #[test]
    fn tracking_nonzero_exit_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(LOCAL_TAG),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("fatal: ref locked", 1),
        ]);

        let outcome = execute(PushTags { repo: ".".into() }, &git)
            .expect("a Git rejection is a closed action failure");

        assert_eq!(
            outcome.detail,
            "git update-ref failed for v1.0.0: fatal: ref locked"
        );
        assert_eq!(
            outcome.progress.remote_push,
            TagRemotePushProgress::Completed {
                pushed_refs: vec!["refs/tags/v1.0.0".into()],
            }
        );
        assert_eq!(outcome.progress.tracking_refs_updated, Vec::<String>::new());
        assert_eq!(
            outcome.progress.tracking_refs_pending,
            vec!["refs/remotes/origin/tags/v1.0.0"]
        );
    }

    #[test]
    fn later_tracking_transport_reports_updated_and_pending_refs() {
        let local_tags = concat!(
            "object-v1\t\t\tv1.0.0\t\t100\n",
            "object-stable\t\t\tstable\t\t101\n",
        );
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok(local_tags)),
            Ok(FakeGitRunner::ok("")),
            Ok(FakeGitRunner::ok("")),
            Ok(FakeGitRunner::ok("")),
            Err(anyhow::anyhow!("tracking transport unavailable")),
        ]);

        let error = execute(PushTags { repo: ".".into() }, &git)
            .expect_err("second tracking update transport fails");

        let PushTagsError::Unexpected { progress, source } = error;
        assert_eq!(
            progress.remote_push,
            TagRemotePushProgress::Completed {
                pushed_refs: vec!["refs/tags/stable".into(), "refs/tags/v1.0.0".into()],
            }
        );
        assert_eq!(
            progress.tracking_refs_updated,
            vec!["refs/remotes/origin/tags/stable"]
        );
        assert_eq!(
            progress.tracking_refs_pending,
            vec!["refs/remotes/origin/tags/v1.0.0"]
        );
        assert_eq!(source.to_string(), "tracking transport unavailable");
    }

    fn assert_transport_error(error: &(dyn std::error::Error + 'static)) {
        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    fn failed(detail: &str) -> TagActionOutcome {
        TagActionOutcome {
            status: TagActionStatus::Failed,
            detail: detail.into(),
            progress: TagOperationProgress::default(),
        }
    }
}
