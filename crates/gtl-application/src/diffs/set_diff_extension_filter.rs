use std::{collections::BTreeSet, sync::Arc};

use gtl_models::diffs::{AppliedExtensionFilter, ExtensionFilter};

use super::{
    FullContextDiff, FullContextDiffSource, FullContextDiffState, View, unified_diff,
    view::{attach_full_context, sort_files_tree_order},
};
use crate::ports::{GitClient, GitDiffFormat, GitDiffRequest};

pub struct SetDiffExtensionFilter {
    pub view: Arc<View>,
    pub filter: ExtensionFilter,
}

#[derive(Debug, thiserror::Error)]
pub enum SetDiffExtensionFilterError {
    #[error("the diff source is unavailable")]
    SourceUnavailable,
    #[error(transparent)]
    Git(#[from] anyhow::Error),
}

#[cqrsy::command]
pub fn execute(
    request: SetDiffExtensionFilter,
    git: &impl GitClient,
) -> Result<View, SetDiffExtensionFilterError> {
    let SetDiffExtensionFilter { view, filter } = request;
    let mut result = (*view).clone();
    let mut hidden_paths = view
        .extension_filter
        .as_ref()
        .map(|value| value.hidden_paths.iter().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let loaded_hidden_paths = view
        .file_filter
        .hidden_files
        .iter()
        .map(|file| &file.path)
        .collect::<BTreeSet<_>>();
    let missing = hidden_paths
        .iter()
        .filter(|path| !filter.hides(path) && !loaded_hidden_paths.contains(path))
        .cloned()
        .collect::<BTreeSet<_>>();
    if !missing.is_empty() {
        let spec = view
            .file_filter
            .source
            .clone()
            .ok_or(SetDiffExtensionFilterError::SourceUnavailable)?;
        let mut request = GitDiffRequest {
            spec,
            format: GitDiffFormat::Unified,
            paths: filter
                .shown()
                .intersection(&view.file_filter.filter.hidden()),
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
        .partition(|file| filter.hides(&file.path));
    hidden_paths.retain(|path| filter.hides(path));
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
            filter.shown(),
        ));
    }
    result.extension_filter = AppliedExtensionFilter::from_hidden(&filter, hidden_paths);
    result.file_filter.filter = filter;
    sort_files_tree_order(&mut result.files);
    Ok(result)
}
