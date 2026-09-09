mod weight;

use std::collections::{HashMap, VecDeque};

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerActiveView, ViewerRowContentId};
use lru::LruCache;

use super::client_diff::{
    ClientDiffFileError, ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ClientDiffWindow,
    ClientDiffWorkspace, ClientDiffWorkspaceStoreExt, LoadedRowWindow, window_too_large,
};

const RETAINED_VIEWS_MAX: usize = 8;
pub(super) const RETAINED_ROW_BYTES_MAX: usize = 64 * 1024 * 1024;
const RETAINED_WINDOWS_MAX: usize = 4_096;

pub(super) use weight::{row_window_bytes, split_row_bytes, unified_row_bytes};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct WindowKey {
    content: ClientDiffCacheKey,
    window: ClientDiffWindow,
}

struct RetainedRows {
    windows: LruCache<WindowKey, usize>,
    bytes: usize,
}

impl Default for RetainedRows {
    fn default() -> Self {
        Self {
            windows: LruCache::unbounded(),
            bytes: 0,
        }
    }
}

impl RetainedRows {
    fn pop_unprotected(
        &mut self,
        key: &ClientDiffCacheKey,
        protected: &[ClientDiffWindow],
    ) -> Option<WindowKey> {
        let evicted = self
            .windows
            .iter()
            .rev()
            .find(|(candidate, _)| {
                &candidate.content != key || !protected.contains(&candidate.window)
            })
            .map(|(candidate, _)| candidate.clone())?;
        self.bytes -= self.windows.pop(&evicted).unwrap_or_default();
        Some(evicted)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ClientDiffCacheKey {
    pub(super) server_instance_id: Option<String>,
    pub(super) content_id: ViewerRowContentId,
}

#[derive(Clone, Copy)]
pub(crate) struct ClientDiffCache {
    workspaces: Store<HashMap<ClientDiffCacheKey, ClientDiffWorkspace>>,
    recency: Signal<VecDeque<ClientDiffCacheKey>>,
    rows: Signal<RetainedRows>,
}

pub(crate) fn use_client_diff_cache_provider() {
    let workspaces = use_store(HashMap::new);
    let recency = use_signal(VecDeque::new);
    let rows = use_signal(RetainedRows::default);
    use_context_provider(|| ClientDiffCache {
        workspaces,
        recency,
        rows,
    });
}

impl ClientDiffCache {
    pub(super) fn select(
        mut self,
        server_instance_id: Option<String>,
        view: &ViewerActiveView,
    ) -> Store<ClientDiffWorkspace> {
        let expired = self
            .recency
            .peek()
            .iter()
            .filter(|key| key.server_instance_id != server_instance_id)
            .cloned()
            .collect::<Vec<_>>();
        for key in expired {
            self.forget_windows(&key);
            self.workspaces.remove(&key);
        }
        self.recency
            .write()
            .retain(|key| key.server_instance_id == server_instance_id);
        let key = ClientDiffCacheKey {
            server_instance_id,
            content_id: view.content_id,
        };
        if !self.workspaces.peek().contains_key(&key) {
            self.workspaces.insert(
                key.clone(),
                ClientDiffWorkspace::loading(view.identity, view.files.clone()),
            );
        }
        let workspace: Store<ClientDiffWorkspace> =
            self.workspaces.get_unchecked(key.clone()).into();
        rebind_workspace(workspace, view);
        let mut recency = self.recency.write();
        recency.retain(|entry| entry != &key);
        recency.push_back(key);
        drop(recency);
        self.trim();
        workspace
    }

    fn trim(mut self) {
        let mut recency_signal = self.recency;
        let mut recency = recency_signal.write();
        while recency.len() > RETAINED_VIEWS_MAX
            && let Some(evicted) = recency.pop_front()
        {
            self.forget_windows(&evicted);
            self.workspaces.remove(&evicted);
        }
    }

    fn forget_windows(mut self, key: &ClientDiffCacheKey) {
        let expired = self
            .rows
            .peek()
            .windows
            .iter()
            .filter(|(window, _)| &window.content == key)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut rows = self.rows.write();
        for key in expired {
            let bytes = rows.windows.pop(&key).unwrap_or_default();
            rows.bytes -= bytes;
        }
    }

    pub(super) fn contains_window(
        self,
        key: &ClientDiffCacheKey,
        window: ClientDiffWindow,
    ) -> bool {
        self.rows.peek().windows.contains(&WindowKey {
            content: key.clone(),
            window,
        })
    }

    pub(super) fn touch_window(mut self, key: &ClientDiffCacheKey, window: ClientDiffWindow) {
        self.rows.write().windows.get(&WindowKey {
            content: key.clone(),
            window,
        });
    }

    pub(super) fn retain_window(
        mut self,
        key: &ClientDiffCacheKey,
        window: ClientDiffWindow,
        loaded: LoadedRowWindow,
        protected: &[ClientDiffWindow],
    ) -> Result<bool, ClientDiffFileError> {
        self.validate_window(key, window, &loaded)?;
        let bytes = row_window_bytes(&loaded.rows);
        if bytes > RETAINED_ROW_BYTES_MAX {
            return Err(window_too_large());
        }
        if !self.workspaces.peek().contains_key(key) {
            return Err(ClientDiffFileError::InvalidResponse);
        }
        let cache_key = WindowKey {
            content: key.clone(),
            window,
        };
        let mut rows_signal = self.rows;
        let mut retained = rows_signal.write();
        let protected_bytes = protected
            .iter()
            .filter(|candidate| **candidate != window)
            .filter_map(|window| {
                retained.windows.peek(&WindowKey {
                    content: key.clone(),
                    window: *window,
                })
            })
            .sum::<usize>();
        if protected_bytes + bytes > RETAINED_ROW_BYTES_MAX
            || protected.len() >= RETAINED_WINDOWS_MAX
        {
            return Ok(false);
        }
        if let Some(previous) = retained.windows.pop(&cache_key) {
            retained.bytes -= previous;
        }
        while (retained.bytes + bytes > RETAINED_ROW_BYTES_MAX
            || retained.windows.len() >= RETAINED_WINDOWS_MAX)
            && let Some(evicted) = retained.pop_unprotected(key, protected)
        {
            self.clear_window(&evicted);
        }
        drop(retained);
        self.write_window(key, window, loaded)?;
        let mut retained = self.rows.write();
        retained.bytes += bytes;
        retained.windows.put(cache_key, bytes);
        Ok(true)
    }

    fn validate_window(
        self,
        key: &ClientDiffCacheKey,
        window: ClientDiffWindow,
        loaded: &LoadedRowWindow,
    ) -> Result<(), ClientDiffFileError> {
        let workspaces = self.workspaces.peek();
        let workspace = workspaces
            .get(key)
            .ok_or(ClientDiffFileError::InvalidResponse)?;
        let file = workspace
            .files
            .get(window.file)
            .ok_or(ClientDiffFileError::InvalidResponse)?;
        let start = window
            .batch
            .checked_mul(super::client_diff::CLIENT_LINE_BATCH_SIZE)
            .ok_or(ClientDiffFileError::InvalidResponse)?;
        let expected = file
            .summary
            .row_count
            .checked_sub(start)
            .ok_or(ClientDiffFileError::InvalidResponse)?
            .min(super::client_diff::CLIENT_LINE_BATCH_SIZE);
        let (layout, count) = match &loaded.rows {
            gtl_wire::viewer::ViewerRows::Unified(rows) => {
                (gtl_wire::viewer::ViewerDiffLayout::Unified, rows.len())
            }
            gtl_wire::viewer::ViewerRows::Split(rows) => {
                (gtl_wire::viewer::ViewerDiffLayout::Split, rows.len())
            }
        };
        if layout != workspace.identity.render_options.layout
            || count == 0
            || count != expected
            || loaded.line_number_digits == 0
        {
            return Err(ClientDiffFileError::InvalidResponse);
        }
        Ok(())
    }

    fn write_window(
        self,
        key: &ClientDiffCacheKey,
        window: ClientDiffWindow,
        loaded: LoadedRowWindow,
    ) -> Result<(), ClientDiffFileError> {
        let workspace: Store<ClientDiffWorkspace> =
            self.workspaces.get_unchecked(key.clone()).into();
        let file = workspace
            .files()
            .get(window.file)
            .ok_or(ClientDiffFileError::InvalidResponse)?;
        match loaded.rows {
            gtl_wire::viewer::ViewerRows::Unified(rows) => {
                let mut batch = file
                    .rows()
                    .unified()
                    .get(window.batch)
                    .ok_or(ClientDiffFileError::InvalidResponse)?;
                batch.set(rows);
            }
            gtl_wire::viewer::ViewerRows::Split(rows) => {
                let mut batch = file
                    .rows()
                    .split()
                    .get(window.batch)
                    .ok_or(ClientDiffFileError::InvalidResponse)?;
                batch.set(rows);
            }
        }
        if *file.line_number_digits().peek() != loaded.line_number_digits {
            file.line_number_digits().set(loaded.line_number_digits);
        }
        Ok(())
    }

    fn clear_window(self, key: &WindowKey) {
        let workspace: Store<ClientDiffWorkspace> =
            self.workspaces.get_unchecked(key.content.clone()).into();
        let Some(file) = workspace.files().get(key.window.file) else {
            return;
        };
        file.state()
            .set(super::client_diff::ClientDiffFileState::Loading);
        if let Some(mut batch) = file.rows().unified().get(key.window.batch) {
            batch.set(Vec::new());
        }
        if let Some(mut batch) = file.rows().split().get(key.window.batch) {
            batch.set(Vec::new());
        }
    }
}

fn rebind_workspace(workspace: Store<ClientDiffWorkspace>, view: &ViewerActiveView) {
    if *workspace.identity().peek() == view.identity {
        return;
    }
    workspace.identity().set(view.identity);
    for (file, summary) in workspace.files().iter().zip(&view.files) {
        if *file.summary().peek() != *summary {
            file.summary().set(summary.clone());
        }
    }
}

#[cfg(test)]
mod tests;
