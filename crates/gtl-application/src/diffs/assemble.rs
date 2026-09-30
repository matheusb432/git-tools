use gtl_models::{
    diffs::{Commit, ExtensionFilter, ExtensionSelection},
    git::{GitDiffSpec, GitRange},
    paths::{RepositoryRelativePath, RepositoryRoot},
    timestamps::MachineTimestamp,
};

use crate::{
    diffs::{FileDiff, FileStatus, FullContextDiffSource, FullContextDiffState, unified_diff},
    ports::{GitClient, GitDiffFormat, GitDiffRequest},
};

pub(super) struct DiffData {
    /// The compared endpoints after any changes-since narrowing.
    pub spec: GitDiffSpec,
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
    filter: &ExtensionFilter,
    changes_since: Option<&MachineTimestamp>,
) -> anyhow::Result<DiffData> {
    let commits = log_range
        .map(|range| git.log_commits(repo_path, range))
        .transpose()?
        .unwrap_or_default();
    let (diff_spec, commits) = match changes_since
        .and_then(|cutoff| super::changes_since::narrow(diff_spec, &commits, cutoff))
    {
        Some(narrowed) => (narrowed.spec, narrowed.commits),
        None => (diff_spec.clone(), commits),
    };
    let diff_spec = &diff_spec;

    let hidden_paths = hidden_paths(git, repo_path, diff_spec, filter)?;
    let content_request = GitDiffRequest {
        spec: diff_spec.clone(),
        format: GitDiffFormat::Unified,
        paths: filter.shown(),
    };
    // Enforce the filter even when the Git adapter ignores pathspecs.
    let (files, _) = filter_hidden_files(
        unified_diff::parse(&git.diff(repo_path, &content_request)?)?,
        filter,
    );
    let full_context = if files
        .iter()
        .any(|file| file.status() == FileStatus::Modified)
    {
        FullContextDiffState::Deferred(FullContextDiffSource::new(
            content_request.spec,
            content_request.paths,
        ))
    } else {
        FullContextDiffState::Loaded
    };
    Ok(DiffData {
        spec: diff_spec.clone(),
        commits,
        files,
        hidden_paths,
        full_context,
    })
}

fn filter_hidden_files(
    files: Vec<FileDiff>,
    filter: &ExtensionFilter,
) -> (Vec<FileDiff>, Vec<RepositoryRelativePath>) {
    if !filter.is_active() {
        return (files, Vec::new());
    }
    let (hidden, kept): (Vec<FileDiff>, Vec<FileDiff>) =
        files.into_iter().partition(|file| filter.hides(&file.path));
    (kept, hidden.into_iter().map(|file| file.path).collect())
}

fn hidden_paths(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    spec: &GitDiffSpec,
    filter: &ExtensionFilter,
) -> anyhow::Result<Vec<RepositoryRelativePath>> {
    if !filter.is_active() {
        return Ok(Vec::new());
    }
    Ok(git
        .diff(
            repo_path,
            &GitDiffRequest {
                spec: spec.clone(),
                format: GitDiffFormat::NamesOnly,
                paths: ExtensionSelection::all(),
            },
        )?
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let path = unified_diff::unquote_git_path(line)?;
            RepositoryRelativePath::try_new(path.into()).map_err(anyhow::Error::from)
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| filter.hides(path))
        .collect())
}
