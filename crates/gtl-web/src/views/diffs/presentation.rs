use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::ViewerRowContentId;

use super::{
    client_diff_document::viewport::geometry::DiffGeometry,
    diff_workspace::{
        FileFoldCommand,
        panel_scroll::{Panel, PanelScrollPosition},
    },
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ScrollAnchor {
    pub(super) file: String,
    pub(super) row: Option<u32>,
    pub(super) offset: f64,
}

#[derive(Default)]
struct TabPresentation {
    anchor: Option<ScrollAnchor>,
    files_scroll: PanelScrollPosition,
    commits_scroll: PanelScrollPosition,
    default_expanded: Option<bool>,
    files: HashMap<String, bool>,
    geometry: Option<RetainedGeometry>,
}

struct RetainedGeometry {
    content: ViewerRowContentId,
    wrap_lines: bool,
    width: f64,
    geometry: DiffGeometry,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct DiffPresentation {
    tabs: Signal<HashMap<ViewerTabId, TabPresentation>>,
    pub(crate) fold_command: Signal<Option<FileFoldCommand>>,
}

pub(crate) fn use_diff_presentation_provider() {
    let mut tabs = use_signal(HashMap::<ViewerTabId, TabPresentation>::new);
    let fold_command = use_signal(|| None);
    use_context_provider(|| DiffPresentation { tabs, fold_command });
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
    #[cfg(test)]
    pub(super) fn detached() -> Self {
        Self {
            tabs: Signal::new(HashMap::new()),
            fold_command: Signal::new(None),
        }
    }

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

    pub(crate) fn all_folded(self, tab: ViewerTabId) -> Option<bool> {
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

    pub(crate) fn toggle_files(mut self, tab_id: ViewerTabId) {
        let folded = !self.all_folded(tab_id).unwrap_or(false);
        {
            let mut tabs = self.tabs.write();
            let tab = tabs.entry(tab_id).or_default();
            tab.default_expanded = Some(!folded);
            tab.files.clear();
            tab.geometry = None;
        }
        self.fold_command
            .set(Some(FileFoldCommand { tab_id, folded }));
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestResult, viewer_tab_id};

    fn in_presentation(test: impl FnOnce(DiffPresentation) -> TestResult) -> TestResult {
        let owner = VirtualDom::new(VNode::empty);
        owner.in_scope(ScopeId::ROOT, || test(DiffPresentation::detached()))
    }

    #[test]
    fn folding_targets_one_tab_and_applies_before_its_viewport_mounts() -> TestResult {
        in_presentation(|presentation| {
            let first = viewer_tab_id(1)?;
            let second = viewer_tab_id(2)?;
            presentation.ensure_tab(first);
            presentation.set_file_expanded(first, "src/main.rs".to_owned(), true);
            presentation.toggle_files(second);
            assert_eq!(presentation.all_folded(first), None);
            assert!(presentation.file_expanded(first, "src/main.rs", false));
            assert!(!presentation.file_expanded(second, "src/lib.rs", true));
            assert_eq!(
                (presentation.fold_command)().map(|command| command.tab_id),
                Some(second)
            );
            presentation.toggle_files(second);
            assert!(presentation.file_expanded(second, "src/lib.rs", false));
            assert_eq!(presentation.all_folded(first), None);
            Ok(())
        })
    }

    #[test]
    fn panel_scroll_returns_each_tab_and_panel_to_its_own_position() -> TestResult {
        in_presentation(|presentation| {
            let first = viewer_tab_id(1)?;
            let second = viewer_tab_id(2)?;
            let first_files = PanelScrollPosition::new(0.0, 480.0);
            let first_commits = PanelScrollPosition::new(0.0, 96.0);
            let second_files = PanelScrollPosition::new(12.0, 1_200.0);

            presentation.set_panel_scroll(first, Panel::Files, first_files);
            presentation.set_panel_scroll(first, Panel::Commits, first_commits);
            presentation.set_panel_scroll(second, Panel::Files, second_files);

            assert_eq!(presentation.panel_scroll(first, Panel::Files), first_files);
            assert_eq!(
                presentation.panel_scroll(first, Panel::Commits),
                first_commits
            );
            assert_eq!(
                presentation.panel_scroll(second, Panel::Files),
                second_files
            );
            assert_eq!(
                presentation.panel_scroll(second, Panel::Commits),
                PanelScrollPosition::new(0.0, 0.0)
            );
            Ok(())
        })
    }

    #[test]
    fn panel_scroll_starts_a_new_tab_at_the_origin() -> TestResult {
        in_presentation(|presentation| {
            let scrolled = viewer_tab_id(1)?;
            let opened = viewer_tab_id(2)?;
            presentation.set_panel_scroll(
                scrolled,
                Panel::Files,
                PanelScrollPosition::new(0.0, 480.0),
            );
            presentation.set_panel_scroll(
                scrolled,
                Panel::Commits,
                PanelScrollPosition::new(0.0, 96.0),
            );
            presentation.ensure_tab(opened);

            assert_eq!(
                presentation.panel_scroll(opened, Panel::Files),
                PanelScrollPosition::new(0.0, 0.0)
            );
            assert_eq!(
                presentation.panel_scroll(opened, Panel::Commits),
                PanelScrollPosition::new(0.0, 0.0)
            );
            Ok(())
        })
    }
}
