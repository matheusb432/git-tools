use std::collections::HashMap;

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerActiveView, ViewerRows};

use super::{ClientDiffCache, ClientDiffCacheKey, WindowKey};
use crate::entities::diffs::client_diff::{
    ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffWindow, ClientDiffWorkspaceStoreExt,
    LoadedRowWindow,
};

pub(super) fn file_windows(
    cache: ClientDiffCache,
    key: &ClientDiffCacheKey,
    view: &ViewerActiveView,
) {
    let files = view
        .files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| file.source_id.map(|source| ((source, &file.path), index)))
        .collect::<HashMap<_, _>>();
    let mut candidates = HashMap::new();
    let workspaces = cache.workspaces.peek();
    let rows = cache.rows.peek();
    for (source, _) in &rows.windows {
        let Some(file) = workspaces
            .get(&source.content)
            .and_then(|workspace| workspace.files.get(source.window.file))
        else {
            continue;
        };
        let Some(source_id) = file.summary.source_id else {
            continue;
        };
        let Some(&file) = files.get(&(source_id, &file.summary.path)) else {
            continue;
        };
        candidates
            .entry(ClientDiffWindow {
                file,
                batch: source.window.batch,
            })
            .or_insert_with(|| source.clone());
    }
    drop(rows);
    drop(workspaces);
    for (window, source) in candidates {
        let Some(loaded) = copy_window(cache, &source) else {
            continue;
        };
        if matches!(cache.retain_window(key, window, loaded, &[]), Ok(true))
            && let Some(file) = cache.workspace(key).files().get(window.file)
        {
            file.state().set(ClientDiffFileState::Complete);
        }
    }
}

fn copy_window(cache: ClientDiffCache, source: &WindowKey) -> Option<LoadedRowWindow> {
    if !cache.rows.peek().windows.contains(source) {
        return None;
    }
    let workspaces = cache.workspaces.peek();
    let file = workspaces
        .get(&source.content)?
        .files
        .get(source.window.file)?;
    let batch = source.window.batch;
    let rows = match file.rows.unified.get(batch).filter(|rows| !rows.is_empty()) {
        Some(rows) => ViewerRows::Unified(rows.clone()),
        None => ViewerRows::Split(
            file.rows
                .split
                .get(batch)
                .filter(|rows| !rows.is_empty())?
                .clone(),
        ),
    };
    Some(LoadedRowWindow {
        rows,
        line_number_digits: file.line_number_digits,
    })
}
