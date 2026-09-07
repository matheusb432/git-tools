use gtl_models::paths::RepositoryRoot;

use super::{FullContextDiff, FullContextDiffSource, unified_diff};
use crate::ports::GitClient;

#[derive(Debug)]
pub struct FetchFullContextDiff<'a> {
    repo_root: &'a RepositoryRoot,
    source: &'a FullContextDiffSource,
}

impl<'a> FetchFullContextDiff<'a> {
    #[must_use]
    pub const fn new(repo_root: &'a RepositoryRoot, source: &'a FullContextDiffSource) -> Self {
        Self { repo_root, source }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FetchFullContextDiffError {
    #[error("read full-context Git diff")]
    Git(#[source] anyhow::Error),
    #[error("parse full-context Git diff")]
    Parse(#[source] anyhow::Error),
}

#[cqrsy::query]
pub fn execute(
    request: &FetchFullContextDiff<'_>,
    git: &impl GitClient,
) -> Result<FullContextDiff, FetchFullContextDiffError> {
    let raw = git
        .diff(request.repo_root, request.source.git_request())
        .map_err(FetchFullContextDiffError::Git)?;
    let files = unified_diff::parse(&raw).map_err(FetchFullContextDiffError::Parse)?;
    Ok(FullContextDiff::from_files(files))
}

#[cfg(test)]
mod tests {
    use gtl_models::git::GitDiffSpec;

    use super::*;
    use crate::{
        diffs::fetch_full_context_diff,
        utils::{FakeGitClient, git_range, repository_root},
    };

    fn source() -> FullContextDiffSource {
        FullContextDiffSource::new(GitDiffSpec::Range(git_range("a..b")), Vec::new())
    }

    #[test]
    fn fetches_and_parses_the_full_context_git_format() {
        let repo_root = repository_root("/repo");
        let source = source();
        let request = FetchFullContextDiff::new(&repo_root, &source);
        let git = FakeGitClient {
            diff_output: "diff --git a/compact.txt b/compact.txt\n".into(),
            full_diff_output: "diff --git a/full.txt b/full.txt\n retained context\n".into(),
            ..Default::default()
        };

        let full_context = fetch_full_context_diff::execute(&request, &git).unwrap();

        assert_eq!(full_context.files[0].path.to_string_lossy(), "full.txt");
        assert_eq!(full_context.files[0].lines, [" retained context", ""]);
    }

    #[test]
    fn reports_invalid_full_context_source_as_a_parse_error() {
        let repo_root = repository_root("/repo");
        let source = source();
        let request = FetchFullContextDiff::new(&repo_root, &source);
        let git = FakeGitClient {
            full_diff_output: "diff --git a/../secret b/../secret\n".into(),
            ..Default::default()
        };

        let error = fetch_full_context_diff::execute(&request, &git).unwrap_err();

        assert!(matches!(error, FetchFullContextDiffError::Parse(_)));
    }
}
