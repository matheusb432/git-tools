use gtl_models::{paths::RepositoryRoot, viewer::DiffDensity};

use super::{
    FullContextDiff, FullContextDiffSource, FullContextDiffState, View, fetch_full_context_diff,
    unified_diff,
};
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

pub(super) fn load_for_density(
    view: View,
    density: DiffDensity,
    git: &impl GitClient,
) -> anyhow::Result<View> {
    if density == DiffDensity::Full
        && let FullContextDiffState::Deferred(source) = &view.full_context
    {
        let request = FetchFullContextDiff::new(&view.repo_root, source);
        let full_context = fetch_full_context_diff::execute(&request, git)?;
        return Ok(view.with_full_context(full_context)?);
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use gtl_models::{diffs::ExtensionSelection, git::GitDiffSpec};

    use super::*;
    use crate::{
        diffs::fetch_full_context_diff,
        utils::{FakeGitClient, git_range, repository_root},
    };

    fn source() -> FullContextDiffSource {
        FullContextDiffSource::new(
            GitDiffSpec::Range(git_range("a..b")),
            ExtensionSelection::all(),
        )
    }

    #[test]
    fn fetches_and_parses_the_full_context_git_format() {
        let repo_root = repository_root("//fixture.invalid/repositories/repo");
        let source = source();
        let request = FetchFullContextDiff::new(&repo_root, &source);
        let git = FakeGitClient {
            diff_output: "diff --git a/compact.txt b/compact.txt\n".into(),
            full_diff_output: "diff --git a/full.txt b/full.txt\n retained context\n".into(),
            ..Default::default()
        };

        let full_context = fetch_full_context_diff::execute(&request, &git).unwrap();

        assert_eq!(full_context.files[0].path.to_string_lossy(), "full.txt");
        assert_eq!(
            full_context.files[0].lines.iter().collect::<Vec<_>>(),
            [" retained context", ""]
        );
    }

    #[test]
    fn reports_invalid_full_context_source_as_a_parse_error() {
        let repo_root = repository_root("//fixture.invalid/repositories/repo");
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
