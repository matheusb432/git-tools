use gtl_models::{
    diffs::{Commit, ExcludedExtensions},
    git::{GitDiffSpec, GitRange},
    paths::{RepositoryRelativePath, RepositoryRoot},
};

use crate::{
    diffs::{FileDiff, FileStatus, FullContextDiffSource, FullContextDiffState, unified_diff},
    ports::{GitClient, GitDiffFormat, GitDiffPaths, GitDiffRequest},
};

pub(super) struct DiffData {
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
    pub hidden_paths: Vec<RepositoryRelativePath>,
    pub full_context: FullContextDiffState,
}

pub(super) fn assemble(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    diff_spec: &GitDiffSpec,
    log_range: Option<&GitRange>,
    excluded: &ExcludedExtensions,
) -> anyhow::Result<DiffData> {
    let commits = log_range
        .map(|range| git.log_commits(repo_path, range))
        .transpose()?
        .unwrap_or_default();

    let hidden_paths = hidden_paths(git, repo_path, diff_spec, excluded)?;
    let content_request = GitDiffRequest {
        spec: diff_spec.clone(),
        format: GitDiffFormat::Unified,
        paths: GitDiffPaths::Excluding(hidden_paths.clone()),
    };
    // Enforce exclusions even when the Git adapter ignores pathspecs.
    let (files, _) = filter_excluded_files(
        unified_diff::parse(&git.diff(repo_path, &content_request)?)?,
        excluded,
    );
    let full_context = if files
        .iter()
        .any(|file| file.status() == FileStatus::Modified)
    {
        FullContextDiffState::Deferred(FullContextDiffSource::new(
            content_request.spec,
            hidden_paths.clone(),
        ))
    } else {
        FullContextDiffState::Loaded
    };
    Ok(DiffData {
        commits,
        files,
        hidden_paths,
        full_context,
    })
}

fn filter_excluded_files(
    files: Vec<FileDiff>,
    excluded: &ExcludedExtensions,
) -> (Vec<FileDiff>, Vec<RepositoryRelativePath>) {
    if excluded.is_empty() {
        return (files, Vec::new());
    }
    let (hidden, kept): (Vec<FileDiff>, Vec<FileDiff>) = files
        .into_iter()
        .partition(|file| excluded.matches(&file.path));
    (kept, hidden.into_iter().map(|file| file.path).collect())
}

fn hidden_paths(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    spec: &GitDiffSpec,
    excluded: &ExcludedExtensions,
) -> anyhow::Result<Vec<RepositoryRelativePath>> {
    if excluded.is_empty() {
        return Ok(Vec::new());
    }
    Ok(git
        .diff(
            repo_path,
            &GitDiffRequest {
                spec: spec.clone(),
                format: GitDiffFormat::NamesOnly,
                paths: GitDiffPaths::Excluding(Vec::new()),
            },
        )?
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| RepositoryRelativePath::try_new(path.into()).map_err(anyhow::Error::from))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| excluded.matches(path))
        .collect())
}
