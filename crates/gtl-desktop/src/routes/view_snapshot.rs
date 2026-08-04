use std::sync::{Arc, Mutex};

use gtl_application::{
    diffs::View,
    viewer::{ViewerDocument, ViewerHistoryPage, ViewerSettings, ViewerView},
};

use crate::session::{CommitSelectionSnapshot, ComputeTicket, ViewerSession};

pub(super) const GENERATION_ATTEMPTS_MAX: usize = 3;

#[derive(Debug, Clone)]
pub(super) struct VersionedView {
    pub(super) ticket: ComputeTicket,
    pub(super) view: Arc<View>,
}

#[derive(Debug)]
pub(super) enum RenderError {
    State(String),
    Retry,
    Conflict,
}

impl From<String> for RenderError {
    fn from(value: String) -> Self {
        Self::State(value)
    }
}

impl From<Arc<gtl_preview::PreviewError>> for RenderError {
    fn from(value: Arc<gtl_preview::PreviewError>) -> Self {
        Self::State(value.to_string())
    }
}

pub(super) struct ViewSnapshot {
    pub(super) document: ViewerDocument,
    pub(super) ticket: Option<ComputeTicket>,
    pub(super) revision: u64,
}

pub(super) fn gather(
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    history: ViewerHistoryPage,
    settings: ViewerSettings,
) -> Result<ViewSnapshot, RenderError> {
    let mut session = session
        .lock()
        .map_err(|_| RenderError::State("session lock poisoned".into()))?;
    let tabs = session
        .tabs()
        .map(|entry| entry.tab.clone())
        .collect::<Vec<_>>();
    let active = session.active();
    let ticket = active.and_then(|id| session.current_ticket(id));
    let revision = session.revision();
    let active_view = active.and_then(|id| {
        let kind = session.tab(id)?.tab.kind();
        let range_view = transient
            .filter(|value| Some(value.ticket) == ticket && value.ticket.tab_id == id)
            .map(|value| value.view)
            .or_else(|| session.cached_view_snapshot(id).map(|cached| cached.view))?;
        Some(match session.commit_selection_snapshot(id) {
            CommitSelectionSnapshot::None => {
                ViewerView::new(id, range_view, settings.options(), kind)
            }
            CommitSelectionSnapshot::Pending { sha } => {
                ViewerView::selection_pending(id, range_view, sha, settings.options(), kind)
            }
            CommitSelectionSnapshot::Ready { sha, view } => {
                ViewerView::selected(id, range_view, view, sha, settings.options(), kind)
            }
            CommitSelectionSnapshot::Error { sha, reason } => {
                ViewerView::selection_error(id, range_view, sha, reason, settings.options(), kind)
            }
        })
    });
    drop(session);
    let document =
        ViewerDocument::new(tabs, active, active_view, history, settings).map_err(|error| {
            if matches!(
                error,
                gtl_application::viewer::ViewerDocumentError::ReadyTabMissingView { .. }
            ) {
                RenderError::Retry
            } else {
                RenderError::State(error.to_string())
            }
        })?;
    Ok(ViewSnapshot {
        document,
        ticket,
        revision,
    })
}

pub(super) fn is_current(
    session: &Mutex<ViewerSession>,
    ticket: Option<ComputeTicket>,
    revision: u64,
) -> Result<bool, RenderError> {
    let session = session
        .lock()
        .map_err(|_| RenderError::State("session lock poisoned".into()))?;
    Ok(session.revision() == revision
        && ticket.is_none_or(|ticket| session.current_ticket(ticket.tab_id) == Some(ticket)))
}
