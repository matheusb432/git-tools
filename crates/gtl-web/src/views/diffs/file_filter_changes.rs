use std::time::Duration;

use dioxus::{core::spawn_forever, prelude::*};
use gtl_models::{diffs::ExcludedExtensions, failure::Failure, viewer::ViewerTabId};
use gtl_wire::viewer::{FieldUpdate, file_filters::SetViewerFileFilters};

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::{
        ui::{ToastHandle, use_toast},
        viewer_client::ViewerClientError,
    },
};

const CHANGE_DEBOUNCE: Duration = Duration::from_millis(150);
const RETAINED_CHANGES_MAX: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
struct ChangeTicket(u64);

#[derive(Clone, PartialEq, Eq)]
enum ChangeStatus {
    Queued,
    Writing,
    Finished(u64),
    Failed(ViewerClientError),
}

#[derive(Clone)]
struct FileFilterChange {
    ticket: ChangeTicket,
    tab_id: ViewerTabId,
    exclusions: FieldUpdate<ExcludedExtensions>,
    displayed: ExcludedExtensions,
    status: ChangeStatus,
}

#[derive(Default)]
struct FileFilterChanges {
    entries: Vec<FileFilterChange>,
    revision: u64,
    refresh_epoch: u64,
    running: bool,
}

impl FileFilterChanges {
    fn submit(
        &mut self,
        tab_id: ViewerTabId,
        exclusions: FieldUpdate<ExcludedExtensions>,
        displayed: ExcludedExtensions,
    ) -> Option<bool> {
        self.entries.retain(|entry| entry.tab_id != tab_id);
        if self.entries.len() >= RETAINED_CHANGES_MAX {
            self.entries
                .retain(|entry| !matches!(entry.status, ChangeStatus::Finished(_)));
        }
        if self.entries.len() >= RETAINED_CHANGES_MAX {
            return None;
        }
        self.revision = self.revision.wrapping_add(1);
        self.entries.push(FileFilterChange {
            ticket: ChangeTicket(self.revision),
            tab_id,
            exclusions,
            displayed,
            status: ChangeStatus::Queued,
        });
        let start = !self.running;
        self.running = true;
        Some(start)
    }

    fn next(&self) -> Option<ChangeTicket> {
        self.entries
            .iter()
            .find(|entry| entry.status == ChangeStatus::Queued)
            .map(|entry| entry.ticket)
    }

    fn begin(&mut self, ticket: ChangeTicket) -> Option<FileFilterChange> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.ticket == ticket && entry.status == ChangeStatus::Queued)?;
        entry.status = ChangeStatus::Writing;
        Some(entry.clone())
    }

    fn complete(&mut self, ticket: ChangeTicket, result: Result<(), ViewerClientError>) -> bool {
        self.refresh_epoch = self.refresh_epoch.wrapping_add(1);
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.ticket == ticket) else {
            return false;
        };
        entry.status = match result {
            Ok(()) => ChangeStatus::Finished(self.refresh_epoch),
            Err(error) => ChangeStatus::Failed(error),
        };
        true
    }

    fn displayed(&self, tab_id: ViewerTabId, fetched_epoch: u64) -> Option<&ExcludedExtensions> {
        self.entries
            .iter()
            .find(|entry| entry.tab_id == tab_id)
            .and_then(|entry| match entry.status {
                ChangeStatus::Finished(epoch) if fetched_epoch >= epoch => None,
                _ => Some(&entry.displayed),
            })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FileFilterController {
    changes: Signal<FileFilterChanges>,
    context: ViewerContext,
    toast: ToastHandle,
}

pub(crate) fn use_file_filter_changes_provider() {
    let mut changes = use_signal(FileFilterChanges::default);
    let context = use_context::<ViewerContext>();
    let toast = use_toast();
    let mut previous_server = use_signal(|| None::<String>);
    use_effect(move || {
        let server = context.server_instance_id();
        if previous_server.peek().is_some() && *previous_server.peek() != server {
            changes.set(FileFilterChanges::default());
        }
        previous_server.set(server);
    });
    use_context_provider(|| FileFilterController {
        changes,
        context,
        toast,
    });
}

impl FileFilterController {
    pub(crate) fn refresh_epoch(self) -> u64 {
        self.changes.read().refresh_epoch
    }

    pub(crate) fn displayed(
        self,
        tab_id: ViewerTabId,
        fetched_epoch: u64,
    ) -> Option<ExcludedExtensions> {
        self.changes
            .read()
            .displayed(tab_id, fetched_epoch)
            .cloned()
    }

    pub(crate) fn error(self, tab_id: ViewerTabId) -> Option<ViewerClientError> {
        self.changes
            .read()
            .entries
            .iter()
            .find_map(|entry| match &entry.status {
                ChangeStatus::Failed(error) if entry.tab_id == tab_id => Some(error.clone()),
                _ => None,
            })
    }

    pub(crate) fn submit(
        mut self,
        tab_id: ViewerTabId,
        exclusions: FieldUpdate<ExcludedExtensions>,
        displayed: ExcludedExtensions,
    ) {
        let start = self.changes.write().submit(tab_id, exclusions, displayed);
        let write = async move { run_changes(&mut self).await };
        match start {
            Some(true) => {
                spawn_forever(write);
            }
            Some(false) => {}
            None => self
                .toast
                .error("Too many pending exclusion changes. Try again shortly."),
        }
    }

    pub(crate) fn retry(self, tab_id: ViewerTabId) {
        let entry = self
            .changes
            .peek()
            .entries
            .iter()
            .find(|entry| entry.tab_id == tab_id)
            .cloned();
        if let Some(entry) = entry {
            self.submit(tab_id, entry.exclusions, entry.displayed);
        }
    }
}

async fn run_changes(controller: &mut FileFilterController) {
    let server = controller.context.server_instance_id();
    loop {
        let next = controller.changes.peek().next();
        let Some(ticket) = next else {
            controller.changes.write().running = false;
            return;
        };
        dioxus_sdk_time::sleep(CHANGE_DEBOUNCE).await;
        if controller.context.server_instance_id() != server {
            return;
        }
        let change = controller.changes.write().begin(ticket);
        let Some(change) = change else {
            continue;
        };
        let result = write_change(&change, &controller.context, server.as_deref()).await;
        if controller.context.server_instance_id() != server {
            return;
        }
        let current = controller.changes.write().complete(ticket, result.clone());
        if current && let Err(error) = result {
            controller.toast.client_error(&error);
        }
        controller.context.refresh(false);
    }
}

async fn write_change(
    change: &FileFilterChange,
    context: &ViewerContext,
    server: Option<&str>,
) -> Result<(), ViewerClientError> {
    let current = viewer_server::get_file_filters(gtl_wire::viewer::ViewerTabRequest {
        tab_id: change.tab_id,
    })
    .await?;
    if context.server_instance_id().as_deref() != server {
        return Err(ViewerClientError::Failed(Failure::Changed));
    }
    viewer_server::set_file_filters(SetViewerFileFilters {
        tab_id: change.tab_id,
        exclusions: change.exclusions.clone(),
        expected: current.saved,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestResult, viewer_tab_id};

    fn change(
        queue: &mut FileFilterChanges,
        tab: ViewerTabId,
        extensions: &[&str],
    ) -> TestResult<ChangeTicket> {
        let excluded = ExcludedExtensions::new(extensions.iter().copied());
        queue
            .submit(tab, FieldUpdate::Update(excluded.clone()), excluded)
            .ok_or("queue full")?;
        Ok(queue.entries.last().ok_or("missing change")?.ticket)
    }

    #[test]
    fn rapid_edits_coalesce_before_writing_and_show_the_latest_selection() -> TestResult {
        let mut queue = FileFilterChanges::default();
        let tab = viewer_tab_id(1)?;
        let first = change(&mut queue, tab, &["lock"])?;
        let latest = change(&mut queue, tab, &["lock", "json"])?;
        assert!(queue.begin(first).is_none());
        assert_eq!(
            queue.displayed(tab, 0),
            Some(&ExcludedExtensions::new(["lock", "json"]))
        );
        assert!(queue.begin(latest).is_some());
        assert!(queue.next().is_none());
        Ok(())
    }

    #[test]
    fn stale_completion_cannot_replace_a_newer_edit_or_restore() -> TestResult {
        let mut queue = FileFilterChanges::default();
        let tab = viewer_tab_id(1)?;
        let first = change(&mut queue, tab, &["json"])?;
        queue.begin(first).ok_or("missing first write")?;
        let defaults = ExcludedExtensions::new(["lock"]);
        assert_eq!(
            queue.submit(tab, FieldUpdate::Clear, defaults.clone()),
            Some(false)
        );
        assert!(!queue.complete(first, Err(ViewerClientError::Disconnected)));
        assert_eq!(queue.displayed(tab, queue.refresh_epoch), Some(&defaults));
        let next = queue.next().ok_or("restore lost")?;
        let restore = queue.begin(next).ok_or("restore unavailable")?;
        assert!(matches!(restore.exclusions, FieldUpdate::Clear));
        assert!(queue.complete(next, Ok(())));
        assert_eq!(
            queue.displayed(tab, queue.refresh_epoch - 1),
            Some(&defaults)
        );
        assert!(queue.displayed(tab, queue.refresh_epoch).is_none());
        Ok(())
    }

    #[test]
    fn switching_tabs_preserves_pending_writes_and_failed_edits_remain_retryable() -> TestResult {
        let mut queue = FileFilterChanges::default();
        let first_tab = viewer_tab_id(1)?;
        let other_tab = viewer_tab_id(2)?;
        let first = change(&mut queue, first_tab, &["lock"])?;
        let other = change(&mut queue, other_tab, &["txt"])?;
        assert_eq!(
            queue.begin(first).ok_or("first tab lost")?.tab_id,
            first_tab
        );
        queue.complete(first, Err(ViewerClientError::Disconnected));
        assert_eq!(
            queue.begin(other).ok_or("other tab lost")?.tab_id,
            other_tab
        );
        queue.complete(other, Ok(()));
        assert_eq!(
            queue.displayed(first_tab, queue.refresh_epoch),
            Some(&ExcludedExtensions::new(["lock"]))
        );
        let retry = change(&mut queue, first_tab, &["lock"])?;
        assert!(queue.begin(retry).is_some());
        queue.complete(retry, Ok(()));
        assert!(queue.displayed(first_tab, queue.refresh_epoch).is_none());
        Ok(())
    }
}
