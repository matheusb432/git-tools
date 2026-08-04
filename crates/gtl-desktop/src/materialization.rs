use std::{collections::VecDeque, num::NonZeroU64, sync::Mutex};

use gtl_application::viewer::{RenderOptions, ViewerTabState};
use gtl_preview::ViewChunk;

use crate::session::{CommitSelectionSnapshot, ComputeTicket, ViewerSession};

/// Identifies one active, non-zero view materialization.
///
/// Keeping the raw integer private prevents unvalidated route values from
/// entering the materialization and rendering pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewLoadId(NonZeroU64);

impl ViewLoadId {
    pub(crate) fn try_new(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(Self)
    }

    /// Returns the raw identifier required by the preview renderer boundary.
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

impl std::fmt::Display for ViewLoadId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug)]
struct ActiveMaterialization {
    id: ViewLoadId,
    ticket: ComputeTicket,
    revision: u64,
    chunks: VecDeque<ViewChunk>,
}

#[derive(Debug)]
struct MaterializationState {
    next_id: Option<ViewLoadId>,
    active: Option<ActiveMaterialization>,
}

impl Default for MaterializationState {
    fn default() -> Self {
        Self {
            next_id: Some(ViewLoadId(NonZeroU64::MIN)),
            active: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ChunkPage {
    pub(crate) chunk: ViewChunk,
    pub(crate) has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MaterializationError {
    Conflict,
    ExhaustedIds,
    Render(String),
    StatePoisoned,
}

impl std::fmt::Display for MaterializationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => formatter.write_str("stale view materialization"),
            Self::ExhaustedIds => formatter.write_str("view materialization ids exhausted"),
            Self::Render(reason) => write!(formatter, "view materialization failed: {reason}"),
            Self::StatePoisoned => formatter.write_str("view materialization state poisoned"),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ViewMaterializations {
    state: Mutex<MaterializationState>,
}

impl ViewMaterializations {
    pub(crate) fn prepare(
        &self,
        session: &Mutex<ViewerSession>,
        options: RenderOptions,
    ) -> Result<Option<ViewLoadId>, MaterializationError> {
        let snapshot = {
            let mut session = session
                .lock()
                .map_err(|_| MaterializationError::StatePoisoned)?;
            let Some(tab_id) = session.active() else {
                self.clear()?;
                return Ok(None);
            };
            let Some(tab) = session.tab(tab_id) else {
                return Err(MaterializationError::Conflict);
            };
            if !matches!(tab.tab.state(), ViewerTabState::Ready) {
                self.clear()?;
                return Ok(None);
            }
            let ticket = session
                .current_ticket(tab_id)
                .ok_or(MaterializationError::Conflict)?;
            let cached = session
                .cached_view_snapshot(tab_id)
                .ok_or(MaterializationError::Conflict)?;
            let view = match session.commit_selection_snapshot(tab_id) {
                CommitSelectionSnapshot::Ready { view, .. } => view,
                CommitSelectionSnapshot::None => cached.view,
                CommitSelectionSnapshot::Pending { .. } | CommitSelectionSnapshot::Error { .. } => {
                    self.clear()?;
                    return Ok(None);
                }
            };
            (ticket, session.revision(), view)
        };

        let chunks = gtl_preview::view_chunks(&snapshot.2, options)
            .map_err(|error| MaterializationError::Render(error.to_string()))?;
        {
            let session = session
                .lock()
                .map_err(|_| MaterializationError::StatePoisoned)?;
            if session.active() != Some(snapshot.0.tab_id)
                || session.current_ticket(snapshot.0.tab_id) != Some(snapshot.0)
                || session.revision() != snapshot.1
            {
                return Err(MaterializationError::Conflict);
            }
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| MaterializationError::StatePoisoned)?;
        let id = state.next_id.ok_or(MaterializationError::ExhaustedIds)?;
        state.next_id = id.get().checked_add(1).and_then(ViewLoadId::try_new);
        state.active = Some(ActiveMaterialization {
            id,
            ticket: snapshot.0,
            revision: snapshot.1,
            chunks,
        });
        Ok(Some(id))
    }

    pub(crate) fn next(
        &self,
        session: &Mutex<ViewerSession>,
        id: ViewLoadId,
    ) -> Result<ChunkPage, MaterializationError> {
        let session = session
            .lock()
            .map_err(|_| MaterializationError::StatePoisoned)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| MaterializationError::StatePoisoned)?;
        let materialization = state
            .active
            .as_mut()
            .filter(|materialization| materialization.id == id)
            .ok_or(MaterializationError::Conflict)?;
        if session.active() != Some(materialization.ticket.tab_id)
            || session.current_ticket(materialization.ticket.tab_id) != Some(materialization.ticket)
            || session.revision() != materialization.revision
        {
            state.active = None;
            return Err(MaterializationError::Conflict);
        }
        let chunk = materialization
            .chunks
            .pop_front()
            .ok_or(MaterializationError::Conflict)?;
        let has_more = !materialization.chunks.is_empty();
        if !has_more {
            state.active = None;
        }
        Ok(ChunkPage { chunk, has_more })
    }

    fn clear(&self) -> Result<(), MaterializationError> {
        self.state
            .lock()
            .map_err(|_| MaterializationError::StatePoisoned)?
            .active = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use gtl_application::{
        diffs::{Cmd, FileDiff, Foot, View},
        viewer::{RenderOptions, ViewerTabKind},
    };
    use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource};

    use super::*;
    use crate::session::{CachedView, ViewerSession};

    fn recipe(path: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(path.into()),
            op: RecipeOp::SquashPreview { pinned: None },
            name: None,
        }
    }

    fn view(title: &str) -> Arc<View> {
        Arc::new(View {
            exclusions: None,
            repo_name: "repo".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "main".into(),
            commits: Vec::new(),
            files: vec![FileDiff {
                path: "src/lib.rs".into(),
                added: 1,
                removed: 0,
                lines: vec!["@@ -0,0 +1 @@".into(), "+new".into()],
                full_lines: None,
            }],
            title: title.into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
        })
    }

    #[test]
    fn activating_another_tab_invalidates_the_previous_chunk_chain() {
        let mut session = ViewerSession::new(1024 * 1024);
        let first = session
            .open(recipe("/first"), "first".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let first_ticket = session.begin_compute(first).expect("first ticket");
        session.publish_labeled_if_current(
            first_ticket,
            CachedView::new(view("first")),
            "first".into(),
        );
        let second = session
            .open(recipe("/second"), "second".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let second_ticket = session.begin_compute(second).expect("second ticket");
        session.publish_labeled_if_current(
            second_ticket,
            CachedView::new(view("second")),
            "second".into(),
        );
        session.activate(first);
        let session = Mutex::new(session);
        let materializations = ViewMaterializations::default();
        let first_load = materializations
            .prepare(&session, RenderOptions::DEFAULT)
            .expect("prepare first")
            .expect("ready first");

        session.lock().expect("session").activate(second);

        assert!(matches!(
            materializations.next(&session, first_load),
            Err(MaterializationError::Conflict)
        ));
    }
}
