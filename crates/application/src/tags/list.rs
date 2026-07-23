//! Lists local tags as structured commit groups.

use std::path::PathBuf;

use super::{TagList, git_command_error::GitCommandError, group::group, parse};
use crate::ports::GitRunner;

/// Requests the local tags for one repository, optionally with their origin state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTags {
    pub repo: PathBuf,
    /// Queries origin over the network to resolve each tag's [`domain::tags::TagState`].
    pub include_state: bool,
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
#[cqrsy::query]
pub fn execute(query: ListTags, git: &impl GitRunner) -> Result<TagList, ListTagsError> {
    let ListTags {
        repo,
        include_state,
    } = query;
    let refs = if include_state {
        parse::load(git, &repo)
    } else {
        parse::load_local(git, &repo)
    };
    match refs {
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

        let error =
            execute(list_with_state(), &git).expect_err("transport failure must remain an error");

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
            execute(list_with_state(), &git).expect("a Git rejection is a closed list failure"),
            TagList::Failed {
                detail: "git for-each-ref failed: fatal: refs unavailable".into(),
            }
        );
    }

    #[test]
    fn listing_without_state_never_contacts_origin() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok("")]);

        execute(
            ListTags {
                repo: ".".into(),
                include_state: false,
            },
            &git,
        )
        .expect("scripted git succeeds");

        let calls = git.arg_lists();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0][0], "for-each-ref");
    }

    #[test]
    fn unreachable_origin_is_a_closed_failure_naming_the_remote_query() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err(
                "fatal: 'origin' does not appear to be a git repository",
                128,
            ),
        ]);

        assert_eq!(
            execute(list_with_state(), &git).expect("a Git rejection is a closed list failure"),
            TagList::Failed {
                detail:
                    "git ls-remote failed: fatal: 'origin' does not appear to be a git repository"
                        .into(),
            }
        );
    }

    fn list_with_state() -> ListTags {
        ListTags {
            repo: ".".into(),
            include_state: true,
        }
    }
}
