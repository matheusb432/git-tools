//! Lists local tags as structured commit groups.

use std::path::PathBuf;

use super::{TagList, git_command_error::GitCommandError, group::group, parse};
use crate::ports::GitRunner;

/// Requests the local and origin-tracking tag state for one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTags {
    pub repo: PathBuf,
}

/// Reports an unexpected Git transport failure while listing tags.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ListTagsError {
    /// Git could not be started or its output could not be collected.
    #[error("{0}")]
    Unexpected(#[source] anyhow::Error),
}

/// Loads and groups local tags without applying presentation choices.
///
/// # Errors
///
/// Returns [`ListTagsError`] when Git cannot be executed.
#[cqrsy::handler(query)]
pub fn execute(query: ListTags, git: &impl GitRunner) -> Result<TagList, ListTagsError> {
    let ListTags { repo } = query;
    match parse::load(git, &repo) {
        Ok(refs) => Ok(TagList::Listed {
            groups: group(refs.into_listed()),
        }),
        Err(GitCommandError::Rejected { detail, .. }) => Ok(TagList::Failed { detail }),
        Err(GitCommandError::Transport { source, .. }) => Err(ListTagsError::Unexpected(source)),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{ListTags, execute};
    use crate::{tags::TagList, testing::FakeGitRunner};

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(ListTags { repo: ".".into() }, &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    #[test]
    fn nonzero_exit_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::exit_err(
            "fatal: refs unavailable",
            128,
        )]);

        assert_eq!(
            execute(ListTags { repo: ".".into() }, &git)
                .expect("a Git rejection is a closed list failure"),
            TagList::Failed {
                detail: "git for-each-ref failed: fatal: refs unavailable".into(),
            }
        );
    }
}
