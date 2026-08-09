use std::{collections::VecDeque, num::NonZeroU64, sync::Mutex};

use gtl_application::viewer::RenderOptions;
use gtl_preview::ViewChunk;

use crate::session::{ActiveContentIdentity, ActiveContentSnapshot, ViewerSession};

/// Identifies one active, non-zero view materialization.
///
/// Keeping the raw integer private prevents unvalidated bridge values from
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

#[derive(Debug)]
struct ActiveMaterialization {
    id: ViewLoadId,
    identity: MaterializationIdentity,
    chunks: VecDeque<ViewChunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MaterializationIdentity {
    content: ActiveContentIdentity,
    options: RenderOptions,
}

#[derive(Debug)]
pub(crate) struct RenderedMaterialization {
    identity: MaterializationIdentity,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedMaterialization {
    Complete,
    Loading(ViewLoadId),
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
    pub(crate) fn render_content(
        snapshot: &ActiveContentSnapshot,
        options: RenderOptions,
    ) -> Result<RenderedMaterialization, MaterializationError> {
        let chunks = gtl_preview::view_chunks(snapshot.view(), options)
            .map_err(|error| MaterializationError::Render(error.to_string()))?;
        Ok(RenderedMaterialization {
            identity: MaterializationIdentity {
                content: snapshot.identity(),
                options,
            },
            chunks,
        })
    }

    pub(crate) fn publish_content(
        &self,
        session: &Mutex<ViewerSession>,
        rendered: RenderedMaterialization,
        options_current: RenderOptions,
    ) -> Result<PreparedMaterialization, MaterializationError> {
        let RenderedMaterialization { identity, chunks } = rendered;
        let session = session
            .lock()
            .map_err(|_| MaterializationError::StatePoisoned)?;
        if identity.options != options_current
            || !session.active_content_is_current(identity.content)
        {
            return Err(MaterializationError::Conflict);
        }
        // Hold the session guard through replacement so the checked content cannot change.
        let mut state = self
            .state
            .lock()
            .map_err(|_| MaterializationError::StatePoisoned)?;
        if chunks.is_empty() {
            state.active = None;
            return Ok(PreparedMaterialization::Complete);
        }
        let id = state.next_id.ok_or(MaterializationError::ExhaustedIds)?;
        state.next_id = id.get().checked_add(1).and_then(ViewLoadId::try_new);
        state.active = Some(ActiveMaterialization {
            id,
            identity,
            chunks,
        });
        Ok(PreparedMaterialization::Loading(id))
    }

    pub(crate) fn next(
        &self,
        session: &Mutex<ViewerSession>,
        id: ViewLoadId,
        options: RenderOptions,
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
        if materialization.identity.options != options {
            return Err(MaterializationError::Conflict);
        }
        if !session.active_content_is_current(materialization.identity.content) {
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
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use gtl_application::{
        diffs::{Cmd, FileDiff, Foot, View},
        viewer::{DiffDensity, DiffLayout, RenderOptions, ViewerTabKind},
    };
    use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource};
    use gtl_models::diffs::Commit;

    use super::*;
    use crate::session::{CachedView, ViewerSession};

    fn recipe(path: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(path.into()),
            op: RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            name: None,
        }
    }

    fn view(title: &str) -> Arc<View> {
        view_with_line(title, "+new")
    }

    fn view_with_line(title: &str, line: &str) -> Arc<View> {
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
                lines: vec!["@@ -0,0 +1 @@".into(), line.into()],
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

    fn publish_ready(
        session: &mut ViewerSession,
        tab: gtl_models::viewer::ViewerTabId,
        view: Arc<View>,
    ) {
        let ticket = session.begin_compute(tab).expect("tab exists");
        assert_eq!(
            session.publish_labeled_if_current(ticket, CachedView::new(view), "ready".into()),
            crate::session::PublishOutcome::Published
        );
    }

    fn two_ready_tabs() -> (Mutex<ViewerSession>, gtl_models::viewer::ViewerTabId) {
        let mut session = ViewerSession::new(1024 * 1024);
        let first = session
            .open(recipe("/first"), "first".into(), ViewerTabKind::Snapshot)
            .expect("first tab id should be available");
        publish_ready(&mut session, first, view_with_line("first", "+first"));
        let second = session
            .open(recipe("/second"), "second".into(), ViewerTabKind::Snapshot)
            .expect("second tab id should be available");
        publish_ready(&mut session, second, view_with_line("second", "+second"));
        assert!(session.activate(first));
        (Mutex::new(session), second)
    }

    fn active_snapshot(session: &Mutex<ViewerSession>) -> ActiveContentSnapshot {
        session
            .lock()
            .expect("session")
            .active_content_snapshot()
            .expect("active content")
    }

    fn loading_id(prepared: PreparedMaterialization) -> ViewLoadId {
        match prepared {
            PreparedMaterialization::Loading(load) => load,
            PreparedMaterialization::Complete => panic!("view contains diff rows"),
        }
    }

    fn prepare_content(
        materializations: &ViewMaterializations,
        session: &Mutex<ViewerSession>,
        snapshot: &ActiveContentSnapshot,
        options: RenderOptions,
    ) -> Result<PreparedMaterialization, MaterializationError> {
        let rendered = ViewMaterializations::render_content(snapshot, options)?;
        materializations.publish_content(session, rendered, options)
    }

    #[test]
    fn an_empty_chunk_stream_completes_without_consuming_a_load_id() {
        let mut session = ViewerSession::new(1024 * 1024);
        let tab = session
            .open(recipe("/repo"), "batch".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let mut empty = (*view("empty")).clone();
        empty.files.clear();
        publish_ready(&mut session, tab, Arc::new(empty));
        let session = Mutex::new(session);
        let materializations = ViewMaterializations::default();
        let empty_snapshot = active_snapshot(&session);

        assert_eq!(
            prepare_content(
                &materializations,
                &session,
                &empty_snapshot,
                RenderOptions::DEFAULT,
            )
            .expect("prepare empty"),
            PreparedMaterialization::Complete
        );

        publish_ready(&mut session.lock().expect("session"), tab, view("nonempty"));
        let nonempty_snapshot = active_snapshot(&session);
        assert_eq!(
            prepare_content(
                &materializations,
                &session,
                &nonempty_snapshot,
                RenderOptions::DEFAULT,
            )
            .expect("prepare nonempty"),
            PreparedMaterialization::Loading(ViewLoadId::try_new(1).expect("first load id"))
        );
    }

    #[test]
    fn a_ready_commit_selection_materializes_the_selected_view() {
        let mut session = ViewerSession::new(1024 * 1024);
        let tab = session
            .open(recipe("/repo"), "batch".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let sha = "a".repeat(40);
        let mut range = (*view_with_line("range", "+range")).clone();
        range.commits = vec![Commit {
            sha: sha.clone(),
            subject: "selected commit".into(),
            ..Default::default()
        }];
        publish_ready(&mut session, tab, Arc::new(range));
        let (ticket, _, _) = session
            .begin_commit_selection(tab, &sha)
            .expect("commit can be selected");
        assert_eq!(
            session
                .publish_commit_patch_if_current(ticket, view_with_line("selected", "+selected")),
            crate::session::PublishOutcome::Published
        );
        let session = Mutex::new(session);
        let materializations = ViewMaterializations::default();
        let snapshot = active_snapshot(&session);

        let load = match prepare_content(
            &materializations,
            &session,
            &snapshot,
            RenderOptions::DEFAULT,
        )
        .expect("prepare selection")
        {
            PreparedMaterialization::Loading(load) => load,
            PreparedMaterialization::Complete => panic!("selected view contains diff rows"),
        };
        let page = materializations
            .next(&session, load, RenderOptions::DEFAULT)
            .expect("selected chunk");

        assert!(page.chunk.html.contains("selected"));
        assert!(!page.chunk.html.contains("+range"));
    }

    #[test]
    fn a_commit_selection_error_materializes_the_range_view() {
        let mut session = ViewerSession::new(1024 * 1024);
        let tab = session
            .open(recipe("/repo"), "batch".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let sha = "a".repeat(40);
        let mut range = (*view_with_line("range", "+range")).clone();
        range.commits = vec![Commit {
            sha: sha.clone(),
            subject: "selected commit".into(),
            ..Default::default()
        }];
        publish_ready(&mut session, tab, Arc::new(range));
        let (ticket, _, _) = session
            .begin_commit_selection(tab, &sha)
            .expect("commit can be selected");
        assert_eq!(
            session.set_commit_patch_error_if_current(ticket, "render failed".into()),
            crate::session::PublishOutcome::Published
        );
        let session = Mutex::new(session);
        let materializations = ViewMaterializations::default();
        let snapshot = active_snapshot(&session);

        let load = match prepare_content(
            &materializations,
            &session,
            &snapshot,
            RenderOptions::DEFAULT,
        )
        .expect("prepare range fallback")
        {
            PreparedMaterialization::Loading(load) => load,
            PreparedMaterialization::Complete => panic!("range view contains diff rows"),
        };
        let page = materializations
            .next(&session, load, RenderOptions::DEFAULT)
            .expect("range chunk");

        assert!(page.chunk.html.contains("range"));
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
        let first_snapshot = active_snapshot(&session);
        let first_load = match prepare_content(
            &materializations,
            &session,
            &first_snapshot,
            RenderOptions::DEFAULT,
        )
        .expect("prepare first")
        {
            PreparedMaterialization::Loading(load) => load,
            PreparedMaterialization::Complete => panic!("first view contains diff rows"),
        };

        session.lock().expect("session").activate(second);

        assert!(matches!(
            materializations.next(&session, first_load, RenderOptions::DEFAULT),
            Err(MaterializationError::Conflict)
        ));
    }

    #[test]
    fn changing_an_inactive_tab_keeps_the_active_chunk_chain_current() {
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
        let first_snapshot = active_snapshot(&session);
        let first_load = match prepare_content(
            &materializations,
            &session,
            &first_snapshot,
            RenderOptions::DEFAULT,
        )
        .expect("prepare first")
        {
            PreparedMaterialization::Loading(load) => load,
            PreparedMaterialization::Complete => panic!("first view contains diff rows"),
        };

        session
            .lock()
            .expect("session")
            .begin_compute(second)
            .expect("second tab still exists");

        assert!(
            materializations
                .next(&session, first_load, RenderOptions::DEFAULT)
                .is_ok()
        );
    }

    #[test]
    fn stale_snapshot_cannot_replace_the_current_chunk_chain() {
        let (session, second) = two_ready_tabs();
        let stale_snapshot = active_snapshot(&session);
        assert!(session.lock().expect("session").activate(second));
        let current_snapshot = active_snapshot(&session);
        let materializations = ViewMaterializations::default();
        let current_load = loading_id(
            prepare_content(
                &materializations,
                &session,
                &current_snapshot,
                RenderOptions::DEFAULT,
            )
            .expect("prepare current content"),
        );

        assert_eq!(
            prepare_content(
                &materializations,
                &session,
                &stale_snapshot,
                RenderOptions::DEFAULT,
            ),
            Err(MaterializationError::Conflict)
        );

        let current_page = materializations
            .next(&session, current_load, RenderOptions::DEFAULT)
            .expect("current chain remains available");
        assert!(current_page.chunk.html.contains("second"));
        assert!(!current_page.chunk.html.contains("first"));
    }

    #[test]
    fn identity_change_after_render_cannot_replace_the_current_chunk_chain() {
        let (session, second) = two_ready_tabs();
        let stale_snapshot = active_snapshot(&session);
        let materializations = ViewMaterializations::default();
        let stale_rendered =
            ViewMaterializations::render_content(&stale_snapshot, RenderOptions::DEFAULT)
                .expect("render stale content");
        assert!(session.lock().expect("session").activate(second));
        let current_snapshot = active_snapshot(&session);
        let current_load = loading_id(
            prepare_content(
                &materializations,
                &session,
                &current_snapshot,
                RenderOptions::DEFAULT,
            )
            .expect("prepare current content"),
        );

        assert_eq!(
            materializations.publish_content(&session, stale_rendered, RenderOptions::DEFAULT,),
            Err(MaterializationError::Conflict)
        );

        let current_page = materializations
            .next(&session, current_load, RenderOptions::DEFAULT)
            .expect("current chain remains available");
        assert!(current_page.chunk.html.contains("second"));
        assert!(!current_page.chunk.html.contains("first"));
    }

    #[test]
    fn old_render_options_cannot_replace_the_current_options_chain() {
        let (session, _second) = two_ready_tabs();
        let snapshot = active_snapshot(&session);
        let materializations = ViewMaterializations::default();
        let options_old = RenderOptions::DEFAULT;
        let options_current = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
        let rendered_old = ViewMaterializations::render_content(&snapshot, options_old)
            .expect("render old options");
        let rendered_current = ViewMaterializations::render_content(&snapshot, options_current)
            .expect("render current options");
        let current_load = loading_id(
            materializations
                .publish_content(&session, rendered_current, options_current)
                .expect("publish current options"),
        );

        assert_eq!(
            materializations.publish_content(&session, rendered_old, options_current),
            Err(MaterializationError::Conflict)
        );
        assert!(
            materializations
                .next(&session, current_load, options_current)
                .is_ok()
        );
    }
}
