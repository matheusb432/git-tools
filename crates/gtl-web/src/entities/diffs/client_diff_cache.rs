mod weight;

use std::collections::{HashMap, VecDeque};

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerActiveView, ViewerRowContentId};

use super::client_diff::{
    ClientDiffFileStoreExt, ClientDiffWorkspace, ClientDiffWorkspaceStoreExt,
};

const RETAINED_VIEWS_MAX: usize = 8;
const RETAINED_ROW_BYTES_MAX: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ClientDiffCacheKey {
    pub(super) server_instance_id: Option<String>,
    pub(super) content_id: ViewerRowContentId,
}

#[derive(Clone, Copy)]
pub(crate) struct ClientDiffCache {
    workspaces: Store<HashMap<ClientDiffCacheKey, ClientDiffWorkspace>>,
    recency: Signal<VecDeque<ClientDiffCacheKey>>,
}

pub(crate) fn use_client_diff_cache_provider() {
    let workspaces = use_store(HashMap::new);
    let recency = use_signal(VecDeque::new);
    use_context_provider(|| ClientDiffCache {
        workspaces,
        recency,
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
        let weights = self
            .workspaces
            .peek()
            .iter()
            .map(|(key, workspace)| (key.clone(), weight::workspace_bytes(workspace)))
            .collect::<HashMap<_, _>>();
        let mut recency = self.recency.write();
        let mut retained_bytes = recency
            .iter()
            .rev()
            .skip(1)
            .map(|key| weights[key])
            .sum::<usize>();
        while recency.len() > 1
            && (recency.len() > RETAINED_VIEWS_MAX || retained_bytes > RETAINED_ROW_BYTES_MAX)
            && let Some(evicted) = recency.pop_front()
        {
            retained_bytes = retained_bytes.saturating_sub(weights[&evicted]);
            self.workspaces.remove(&evicted);
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
