use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::ViewerRowContentId;

use super::{
    client_diff_document::viewport::geometry::DiffGeometry,
    diff_workspace::panel_scroll::{Panel, PanelScrollPosition},
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ScrollAnchor {
    pub(super) file: String,
    pub(super) row: Option<u32>,
    pub(super) offset: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum LineSide {
    Old,
    New,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct ExpandedLine {
    pub(super) content: ViewerRowContentId,
    pub(super) file: String,
    pub(super) row: usize,
    pub(super) side: LineSide,
}

#[derive(Clone)]
pub(super) struct DiffRowBatch {
    pub(super) tab: ViewerTabId,
    pub(super) content: ViewerRowContentId,
    pub(super) file: Rc<str>,
}

#[derive(Clone)]
pub(super) struct DiffRowPresentation {
    pub(super) batch: DiffRowBatch,
    pub(super) row: usize,
}

pub(super) fn use_diff_row(row: usize) {
    let batch = try_use_context::<DiffRowBatch>();
    use_context_provider(move || batch.map(|batch| DiffRowPresentation { batch, row }));
}

#[derive(Default)]
struct TabPresentation {
    anchor: Option<ScrollAnchor>,
    files_scroll: PanelScrollPosition,
    commits_scroll: PanelScrollPosition,
    default_expanded: Option<bool>,
    files: HashMap<String, bool>,
    expanded_lines: HashSet<ExpandedLine>,
    geometry: Option<RetainedGeometry>,
}

struct RetainedGeometry {
    content: ViewerRowContentId,
    wrap_lines: bool,
    width: f64,
    geometry: DiffGeometry,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct DiffPresentation {
    tabs: Signal<HashMap<ViewerTabId, TabPresentation>>,
}

pub(crate) fn use_diff_presentation_provider() {
    let mut tabs = use_signal(HashMap::<ViewerTabId, TabPresentation>::new);
    use_context_provider(|| DiffPresentation { tabs });
    let viewer = use_context::<crate::app::application_layout::ViewerContext>();
    use_effect(move || {
        let shell = viewer.shell();
        let shell = shell.read();
        let crate::app::application_layout::ViewerShellLoad::Ready(shell) = &*shell else {
            return;
        };
        let open = shell.tabs.iter().map(|tab| tab.id).collect::<HashSet<_>>();
        if tabs.peek().keys().any(|tab| !open.contains(tab)) {
            tabs.write().retain(|tab, _| open.contains(tab));
        }
    });
}

impl DiffPresentation {
    pub(super) fn panel_scroll(self, tab: ViewerTabId, panel: Panel) -> PanelScrollPosition {
        let tabs = self.tabs.peek();
        let Some(tab) = tabs.get(&tab) else {
            return PanelScrollPosition::default();
        };
        match panel {
            Panel::Files => tab.files_scroll,
            Panel::Commits => tab.commits_scroll,
        }
    }

    pub(super) fn set_panel_scroll(
        mut self,
        tab: ViewerTabId,
        panel: Panel,
        position: PanelScrollPosition,
    ) {
        if self.panel_scroll(tab, panel) == position {
            return;
        }
        let mut tabs = self.tabs.write();
        let tab = tabs.entry(tab).or_default();
        match panel {
            Panel::Files => tab.files_scroll = position,
            Panel::Commits => tab.commits_scroll = position,
        }
    }

    pub(super) fn all_folded(self, tab: ViewerTabId) -> Option<bool> {
        self.tabs
            .peek()
            .get(&tab)?
            .default_expanded
            .map(|expanded| !expanded)
    }

    pub(super) fn ensure_tab(mut self, tab: ViewerTabId) {
        if !self.tabs.peek().contains_key(&tab) {
            self.tabs.write().insert(tab, TabPresentation::default());
        }
    }

    pub(super) fn file_expanded(self, tab: ViewerTabId, path: &str, initial: bool) -> bool {
        self.tabs
            .peek()
            .get(&tab)
            .and_then(|tab| tab.files.get(path).copied().or(tab.default_expanded))
            .unwrap_or(initial)
    }

    pub(super) fn set_file_expanded(mut self, tab: ViewerTabId, path: String, expanded: bool) {
        if let Some(tab) = self.tabs.write().get_mut(&tab) {
            tab.files.insert(path, expanded);
        }
    }

    pub(super) fn set_all_expanded(mut self, tab: ViewerTabId, expanded: bool) {
        if let Some(tab) = self.tabs.write().get_mut(&tab) {
            tab.default_expanded = Some(expanded);
            tab.files.clear();
        }
    }

    pub(super) fn anchor(self, tab: ViewerTabId) -> Option<ScrollAnchor> {
        self.tabs
            .peek()
            .get(&tab)
            .and_then(|tab| tab.anchor.clone())
    }

    pub(super) fn set_anchor(mut self, tab: ViewerTabId, anchor: ScrollAnchor) {
        if self
            .tabs
            .peek()
            .get(&tab)
            .and_then(|tab| tab.anchor.as_ref())
            == Some(&anchor)
        {
            return;
        }
        if let Some(tab) = self.tabs.write().get_mut(&tab) {
            tab.anchor = Some(anchor);
        }
    }

    pub(super) fn take_geometry(
        mut self,
        tab: ViewerTabId,
        content: ViewerRowContentId,
        wrap_lines: bool,
    ) -> Option<(f64, DiffGeometry)> {
        let retained = self.tabs.write().get_mut(&tab)?.geometry.take()?;
        (retained.content == content && retained.wrap_lines == wrap_lines)
            .then_some((retained.width, retained.geometry))
    }

    pub(super) fn keep_geometry(
        mut self,
        tab: ViewerTabId,
        content: ViewerRowContentId,
        wrap_lines: bool,
        width: f64,
        geometry: DiffGeometry,
    ) {
        if let Some(tab) = self.tabs.write().get_mut(&tab) {
            tab.geometry = Some(RetainedGeometry {
                content,
                wrap_lines,
                width,
                geometry,
            });
        }
    }

    pub(super) fn line_expanded(self, tab: ViewerTabId, line: &ExpandedLine) -> bool {
        self.tabs
            .peek()
            .get(&tab)
            .is_some_and(|tab| tab.expanded_lines.contains(line))
    }

    pub(super) fn set_line_expanded(
        mut self,
        tab: ViewerTabId,
        line: ExpandedLine,
        expanded: bool,
    ) {
        let mut tabs = self.tabs.write();
        let Some(tab) = tabs.get_mut(&tab) else {
            return;
        };
        if expanded {
            tab.expanded_lines.insert(line);
        } else {
            tab.expanded_lines.remove(&line);
        }
    }
}
