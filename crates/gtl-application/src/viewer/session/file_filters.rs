use std::sync::Arc;

use gtl_models::{diffs::ExtensionFilter, timestamps::MachineTimestamp};

use super::{
    ActiveContentIdentity, CachedView, CommitSelection, CommitSelectionSnapshot, ViewerSession,
    tab_by_id_mut,
};
use crate::{
    diffs::View,
    viewer::{ViewerDiffSnapshot, ViewerTabId, file_filters::FileFiltersError},
};

pub(in crate::viewer) struct FileFilterViews {
    pub identity: ActiveContentIdentity,
    pub expected: Arc<View>,
    pub range: ViewerDiffSnapshot,
    pub selected: Option<ViewerDiffSnapshot>,
    pub modified: Option<ViewerDiffSnapshot>,
}

impl ViewerSession {
    /// Returns the filter a tab applies once its first computation has seeded it.
    pub(in crate::viewer) fn tab_extension_filter(
        &self,
        id: ViewerTabId,
    ) -> Option<ExtensionFilter> {
        self.tab(id).and_then(|tab| tab.extension_filter.clone())
    }

    pub(in crate::viewer) fn tab_changes_since(&self, id: ViewerTabId) -> Option<MachineTimestamp> {
        self.tab(id).and_then(|tab| tab.changes_since.clone())
    }

    /// Records the tab's cutoff; the caller recomputes the tab to apply it.
    pub(in crate::viewer) fn set_changes_since(
        &mut self,
        id: ViewerTabId,
        changes_since: Option<MachineTimestamp>,
    ) -> bool {
        let Some(tab) = tab_by_id_mut(&mut self.tabs, id) else {
            return false;
        };
        tab.changes_since = changes_since;
        true
    }

    pub(in crate::viewer) fn file_filter_views(
        &mut self,
        id: ViewerTabId,
    ) -> Option<FileFilterViews> {
        let active = self.content_snapshot(id)?;
        let id = active.identity.tab_id;
        let cached = self.cache.get(id)?.clone();
        let selected = match self.commit_selection_snapshot(id) {
            CommitSelectionSnapshot::Ready { view, .. } => Some(view),
            CommitSelectionSnapshot::Pending { .. } => return None,
            _ => None,
        };
        let mut views = FileFilterViews {
            identity: active.identity,
            expected: active.shared_view(),
            range: cached.view,
            selected,
            modified: self.modified_files_snapshot(id),
        };
        if let Some(full) = self.full_context_snapshot(active.identity) {
            if views.modified.is_some() {
                views.modified = Some(full);
            } else if views.selected.is_some() {
                views.selected = Some(full);
            } else {
                views.range = full;
            }
        }
        Some(views)
    }

    pub(in crate::viewer) fn publish_file_filters(
        &mut self,
        views: FileFilterViews,
        filter: ExtensionFilter,
    ) -> Result<(), FileFiltersError> {
        if self.content_identity(views.identity.tab_id) != Some(views.identity)
            || !self
                .content_snapshot(views.identity.tab_id)
                .is_some_and(|current| Arc::ptr_eq(&current.view, &views.expected))
        {
            return Err(FileFiltersError::Changed);
        }
        let id = views.identity.tab_id;
        let Some(previous) = self.cache.get(id).cloned() else {
            return Err(FileFiltersError::Changed);
        };
        let mut candidate = CachedView::from_snapshot(views.range);
        if let Some(selected) = &views.selected {
            candidate = candidate.with_selected(selected.clone());
        }
        candidate = candidate.with_modified(views.modified);
        if self.cache.insert(id, candidate) == super::CacheDisposition::Oversize {
            self.cache.insert(id, previous);
            return Err(FileFiltersError::TooLarge);
        }
        let Some(tab) = tab_by_id_mut(&mut self.tabs, id) else {
            return Err(FileFiltersError::Changed);
        };
        tab.extension_filter = Some(filter);
        tab.generation = tab.generation.next();
        tab.selection_generation = tab.selection_generation.next();
        if let CommitSelection::Ready { transient, .. } = &mut tab.selection {
            *transient = None;
        }
        if self
            .full_context_transient
            .as_ref()
            .is_some_and(|(identity, _)| identity.tab_id == id)
        {
            self.full_context_transient = None;
        }
        if self
            .modified_files_transient
            .as_ref()
            .is_some_and(|(tab_id, _)| *tab_id == id)
        {
            self.modified_files_transient = None;
        }
        self.bump_version();
        Ok(())
    }
}
