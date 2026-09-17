use std::{collections::BTreeSet, sync::Arc};

use gtl_models::diffs::{AppliedExclusions, ExcludedExtensions};

use super::{
    FullContextDiff, FullContextDiffSource, FullContextDiffState, View, unified_diff,
    view::{attach_full_context, sort_files_tree_order},
};
use crate::ports::{GitClient, GitDiffFormat, GitDiffPaths, GitDiffRequest};

pub struct SetDiffFileExclusions {
    pub view: Arc<View>,
    pub excluded: ExcludedExtensions,
}

#[derive(Debug, thiserror::Error)]
pub enum SetDiffFileExclusionsError {
    #[error("the diff source is unavailable")]
    SourceUnavailable,
    #[error(transparent)]
    Git(#[from] anyhow::Error),
}

#[cqrsy::command]
pub fn execute(
    request: SetDiffFileExclusions,
    git: &impl GitClient,
) -> Result<View, SetDiffFileExclusionsError> {
    let SetDiffFileExclusions { view, excluded } = request;
    let mut result = (*view).clone();
    let mut hidden_paths = view
        .exclusions
        .as_ref()
        .map(|value| value.hidden_paths.iter().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let missing = hidden_paths
        .iter()
        .filter(|path| !excluded.matches(path))
        .filter(|path| {
            !view
                .file_filter
                .hidden_files
                .iter()
                .any(|file| file.path == **path)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        let spec = view
            .file_filter
            .source
            .clone()
            .ok_or(SetDiffFileExclusionsError::SourceUnavailable)?;
        let mut request = GitDiffRequest {
            spec,
            format: GitDiffFormat::Unified,
            paths: GitDiffPaths::Including(missing.clone()),
        };
        let mut files = unified_diff::parse(&git.diff(&view.repo_root, &request)?)?;
        files.retain(|file| missing.contains(&file.path));
        if matches!(view.full_context, FullContextDiffState::Loaded)
            && files
                .iter()
                .any(|file| file.status() == super::FileStatus::Modified)
        {
            request.format = GitDiffFormat::FullContext;
            let full = unified_diff::parse(&git.diff(&view.repo_root, &request)?)?;
            attach_full_context(&mut files, FullContextDiff::from_files(full));
        }
        result.files.extend(files);
    }
    result.files.append(&mut result.file_filter.hidden_files);
    (result.file_filter.hidden_files, result.files) = result
        .files
        .into_iter()
        .partition(|file| excluded.matches(&file.path));
    hidden_paths.retain(|path| excluded.matches(path));
    hidden_paths.extend(
        result
            .file_filter
            .hidden_files
            .iter()
            .map(|file| file.path.clone()),
    );
    let hidden_paths = hidden_paths.into_iter().collect::<Vec<_>>();
    if let FullContextDiffState::Deferred(source) = &view.full_context {
        result.full_context = FullContextDiffState::Deferred(FullContextDiffSource::new(
            source.spec().clone(),
            hidden_paths.clone(),
        ));
    }
    result.exclusions = AppliedExclusions::from_hidden(&excluded, hidden_paths);
    result.file_filter.excluded = excluded;
    sort_files_tree_order(&mut result.files);
    Ok(result)
}
