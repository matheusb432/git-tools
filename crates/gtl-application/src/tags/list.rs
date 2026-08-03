//! Lists local tags as structured commit groups.

use std::path::PathBuf;

use super::{ListTagsOk, git_command_error::GitCommandError, group::group, parse};
use crate::ports::GitClient;

/// Requests the local tags for one repository, optionally with their origin state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTags {
    pub repo_path: PathBuf,
    /// Queries origin over the network to resolve each tag's [`gtl_models::tags::TagState`].
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
pub fn execute(query: ListTags, git: &impl GitClient) -> Result<ListTagsOk, ListTagsError> {
    let ListTags {
        repo_path,
        include_state,
    } = query;
    let refs = if include_state {
        parse::load(git, &repo_path)
    } else {
        parse::load_local(git, &repo_path)
    };
    match refs {
        Ok(refs) => Ok(ListTagsOk::Listed {
            groups: group(refs.into_listed()),
        }),
        Err(GitCommandError::Rejected { detail, .. }) => Ok(ListTagsOk::Failed { detail }),
        Err(GitCommandError::Transport { source, .. }) => Err(ListTagsError::Unexpected(source)),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{ListTags, execute};
    use crate::{tags::ListTagsOk, testing::ScriptedGitClient};

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

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
        let git =
            ScriptedGitClient::new(vec![ScriptedGitClient::rejected("fatal: refs unavailable")]);

        assert_eq!(
            execute(list_with_state(), &git).expect("a Git rejection is a closed list failure"),
            ListTagsOk::Failed {
                detail: "git for-each-ref failed: fatal: refs unavailable".into(),
            }
        );
    }

    #[test]
    fn listing_without_state_never_contacts_origin() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

        execute(
            ListTags {
                repo_path: ".".into(),
                include_state: false,
            },
            &git,
        )
        .expect("scripted git succeeds");
    }

    #[test]
    fn unreachable_origin_is_a_closed_failure_naming_the_remote_query() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("fatal: 'origin' does not appear to be a git repository"),
        ]);

        assert_eq!(
            execute(list_with_state(), &git).expect("a Git rejection is a closed list failure"),
            ListTagsOk::Failed {
                detail:
                    "git ls-remote failed: fatal: 'origin' does not appear to be a git repository"
                        .into(),
            }
        );
    }

    fn list_with_state() -> ListTags {
        ListTags {
            repo_path: ".".into(),
            include_state: true,
        }
    }
}
