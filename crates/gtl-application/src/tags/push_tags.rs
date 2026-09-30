//! Publishes local tags that origin does not already hold.

use std::collections::BTreeSet;

use gtl_models::{
    git::{GitRefName, RemoteName, TagName},
    paths::RepositoryRoot,
    tags::Tag,
};

use super::{
    git_command_error::GitCommandError,
    outcome::{TagActionOutcome, TagOperationProgress},
    refs,
};
use crate::ports::GitClient;

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
pub fn execute(
    repo_path: &RepositoryRoot,
    git: &impl GitClient,
) -> Result<TagActionOutcome, PushTagsError> {
    match push(repo_path, git) {
        Ok(outcome) => Ok(outcome),
        Err(GitCommandError::Rejected { detail, progress }) => {
            Ok(TagActionOutcome::failed_with_progress(detail, *progress))
        }
        Err(GitCommandError::Transport { source, progress }) => Err(PushTagsError::Unexpected {
            progress: *progress,
            source,
        }),
    }
}

fn push(
    repo_path: &RepositoryRoot,
    git: &impl GitClient,
) -> Result<TagActionOutcome, GitCommandError> {
    let refs = refs::load(git, repo_path)?;
    let pending = refs.pending().into_iter().cloned().collect::<Vec<_>>();
    if pending.is_empty() {
        return Ok(TagActionOutcome::noop(
            "tags already up to date",
            BTreeSet::new(),
        ));
    }

    push_tags(git, repo_path, &pending, TagOperationProgress::default())
}

pub(super) fn push_named(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    names: &[TagName],
    created: &TagActionOutcome,
) -> Result<TagActionOutcome, GitCommandError> {
    let prior_progress = created.progress().clone();
    let refs = refs::load(git, repo_path)
        .map_err(|error| error.with_prior_progress(prior_progress.clone()))?;

    let mut pending = Vec::new();
    for name in names {
        let Some(tag) = refs.local(name) else {
            return Ok(TagActionOutcome::failed_with_progress(
                format!("created tag {name} was not found"),
                prior_progress,
            ));
        };
        if !refs.is_remote(tag) {
            pending.push(tag.clone());
        }
    }

    if pending.is_empty() {
        return Ok(TagActionOutcome::noop(
            created.detail(),
            prior_progress.created_refs,
        ));
    }

    let created_detail = created.detail().to_owned();
    Ok(push_tags(git, repo_path, &pending, prior_progress)?.with_created_detail(created_detail))
}

fn push_tags(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    pending: &[Tag],
    mut progress: TagOperationProgress,
) -> Result<TagActionOutcome, GitCommandError> {
    let names = pending
        .iter()
        .map(|tag| tag.name().clone())
        .collect::<Vec<_>>();
    progress.record_push_attempt(&names);
    let references = names
        .iter()
        .map(GitRefName::for_tag)
        .collect::<BTreeSet<_>>();
    let output = git
        .push_tag_refs(repo_path, &RemoteName::origin(), &references)
        .map_err(GitCommandError::from)
        .map_err(|error| error.with_prior_progress(progress.clone()))?;
    if let crate::ports::GitEffect::Rejected(detail) = output {
        return Err(
            GitCommandError::rejected(format!("git push tags failed: {detail}"))
                .with_prior_progress(progress),
        );
    }

    let pushed_refs = names.iter().map(GitRefName::for_tag).collect();
    let created_refs = progress.created_refs;
    let names = pending
        .iter()
        .map(|tag| tag.name().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let noun = if pending.len() == 1 { "tag" } else { "tags" };
    Ok(TagActionOutcome::pushed(
        format!("pushed {} {noun}: {names}", pending.len()),
        created_refs,
        pushed_refs,
    ))
}

#[cfg(test)]
fn push_refspecs(tags: &[Tag]) -> Vec<String> {
    tags.iter()
        .map(|tag| format!("refs/tags/{0}:refs/tags/{0}", tag.name()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use gtl_models::{git::GitRefName, tags::Tag};

    use super::{PushTagsError, push_refspecs};
    use crate::{
        tags::{
            outcome::{TagActionOutcome, TagActionStatus, TagRemotePushProgress},
            push_tags,
        },
        utils::ScriptedGitClient,
    };

    const LOCAL_TAG: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\tv1.0.0\t\t100\n";

    fn tag(name: &str) -> Tag {
        let commit = format!("object-{name}");
        Tag::lightweight(
            crate::utils::tag_name(name),
            crate::utils::commit_id_fixture(&commit),
            None,
        )
    }

    fn refs(names: &[&str]) -> BTreeSet<GitRefName> {
        names
            .iter()
            .map(|name| GitRefName::for_tag(&crate::utils::tag_name(name)))
            .collect()
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
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = push_tags::execute(
            &crate::utils::repository_root("//fixture.invalid/repositories/repo"),
            &git,
        )
        .unwrap_err();

        assert_transport_error(&error);
    }

    #[test]
    fn load_nonzero_exit_remains_the_exact_closed_failure() {
        let git =
            ScriptedGitClient::new(vec![ScriptedGitClient::rejected("fatal: refs unavailable")]);

        assert_eq!(
            push_tags::execute(
                &crate::utils::repository_root("//fixture.invalid/repositories/repo"),
                &git
            )
            .unwrap(),
            failed("git for-each-ref failed: fatal: refs unavailable")
        );
    }

    #[test]
    fn push_transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied(LOCAL_TAG)),
            Ok(ScriptedGitClient::applied("")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = push_tags::execute(
            &crate::utils::repository_root("//fixture.invalid/repositories/repo"),
            &git,
        )
        .unwrap_err();

        let PushTagsError::Unexpected { progress, source } = error;
        assert_eq!(
            progress.remote_push,
            TagRemotePushProgress::Indeterminate {
                attempted_refs: refs(&["v1.0.0"]),
            }
        );
        assert_eq!(source.to_string(), "git transport unavailable");
    }

    #[test]
    fn push_nonzero_exit_remains_the_exact_closed_failure() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(LOCAL_TAG),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("fatal: remote rejected"),
        ]);

        let outcome = push_tags::execute(
            &crate::utils::repository_root("//fixture.invalid/repositories/repo"),
            &git,
        )
        .unwrap();

        assert_eq!(
            outcome.detail(),
            "git push tags failed: fatal: remote rejected"
        );
        assert_eq!(
            outcome.progress().remote_push,
            TagRemotePushProgress::Indeterminate {
                attempted_refs: refs(&["v1.0.0"]),
            }
        );
    }

    #[test]
    fn push_publishes_each_pending_tag_and_writes_no_other_refs() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(LOCAL_TAG),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
        ]);

        let outcome = push_tags::execute(
            &crate::utils::repository_root("//fixture.invalid/repositories/repo"),
            &git,
        )
        .unwrap();

        assert_eq!(outcome.status(), TagActionStatus::Pushed);
        assert_eq!(outcome.detail(), "pushed 1 tag: v1.0.0");
    }

    #[test]
    fn push_is_a_noop_when_origin_already_holds_every_local_tag_object() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(LOCAL_TAG),
            ScriptedGitClient::applied(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trefs/tags/v1.0.0\n",
            ),
        ]);

        let outcome = push_tags::execute(
            &crate::utils::repository_root("//fixture.invalid/repositories/repo"),
            &git,
        )
        .unwrap();

        assert_eq!(outcome.status(), TagActionStatus::Noop);
        assert_eq!(outcome.detail(), "tags already up to date");
    }

    fn assert_transport_error(error: &(dyn std::error::Error + 'static)) {
        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    fn failed(detail: &str) -> TagActionOutcome {
        TagActionOutcome::failed(detail)
    }
}
