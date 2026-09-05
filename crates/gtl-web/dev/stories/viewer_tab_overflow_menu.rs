use std::error::Error;

use dioxus::prelude::*;
use dx_story::{stories, story};
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabKind, ViewerTabState};

use crate::shared::ui::{
    Button, ButtonSize, ButtonVariant, ScrollArea, ScrollAreaVariant, ViewerTabItem,
    ViewerTabOverflowMenu,
};

type PreviewResult<T> = Result<T, Box<dyn Error>>;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    let (tabs, active_tab) = match preview_tabs_with_active() {
        Ok(fixture) => fixture,
        Err(error) => return preview_error(error),
    };

    rsx! {
        div { class: "pointer-events-none flex min-h-24 items-start overflow-hidden border-b border-line bg-surface",
            ViewerTabOverflowMenu {
                id: "preview-tab-overflow-thumbnail",
                tabs,
                active_tab,
                onactivate: move |_| {},
                onclose: move |_| {},
                onmove: move |_| {},
            }
        }
    }
}

/// Interactive collapsed rail with enough tabs to exercise selection and close behavior.
#[story]
fn interactive() -> Element {
    rsx! {
        ViewerTabOverflowDemo { width: PreviewWidth::Desktop }
    }
}

/// The same control constrained to a phone-width viewer rail.
#[story(name = "Narrow rail")]
fn narrow_rail() -> Element {
    rsx! {
        ViewerTabOverflowDemo { width: PreviewWidth::Narrow }
    }
}

/// Seamless tabs with immediate selection, close actions, and drag reordering.
#[story(name = "Interactive tab rail")]
fn tab_rail() -> Element {
    let tabs = match preview_tabs() {
        Ok(tabs) => tabs,
        Err(error) => return preview_error(error),
    };

    rsx! {
        ViewerTabRailDemo { tabs }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewWidth {
    Desktop,
    Narrow,
}

impl PreviewWidth {
    const fn frame_classes(self) -> &'static str {
        match self {
            Self::Desktop => {
                "mx-auto w-full max-w-4xl overflow-hidden rounded-panel border border-line bg-bg shadow-floating"
            }
            Self::Narrow => {
                "mx-auto w-[23rem] max-w-full overflow-hidden rounded-panel border border-line bg-bg shadow-floating"
            }
        }
    }

    const fn frame_label(self) -> &'static str {
        match self {
            Self::Desktop => "Desktop collapsed tab rail preview",
            Self::Narrow => "Narrow collapsed tab rail preview",
        }
    }

    const fn menu_id(self) -> &'static str {
        match self {
            Self::Desktop => "preview-tab-overflow-desktop",
            Self::Narrow => "preview-tab-overflow-narrow",
        }
    }
}

#[component]
fn ViewerTabOverflowDemo(width: PreviewWidth) -> Element {
    let tabs = match preview_tabs() {
        Ok(tabs) => tabs,
        Err(error) => return preview_error(error),
    };

    rsx! {
        ViewerTabOverflowDemoReady { width, tabs }
    }
}

#[component]
fn ViewerTabOverflowDemoReady(width: PreviewWidth, tabs: Vec<ViewerTab>) -> Element {
    let tabs_reset = tabs.clone();
    let mut preview_state = use_signal(move || PreviewTabState::new(tabs));
    let state = preview_state();
    let active_tab = state.active_tab().cloned();
    let active_label = active_tab
        .as_ref()
        .map_or_else(|| "No open diff".to_owned(), |tab| tab.label.clone());

    rsx! {
        section { class: width.frame_classes(), aria_label: width.frame_label(),
            nav {
                class: "flex min-w-0 items-end border-b border-line bg-surface",
                aria_label: "Viewer navigation preview",
                if let Some(active_tab) = active_tab {
                    ViewerTabOverflowMenu {
                        id: width.menu_id(),
                        tabs: state.tabs.clone(),
                        active_tab,
                        diff_rows_loading_tab_id: state.tabs.iter().find(|tab| tab.state == ViewerTabState::Pending).map(|tab| tab.id),
                        onactivate: move |tab_id| {
                            preview_state.set(preview_state().activate(tab_id));
                        },
                        onclose: move |tab_id| {
                            preview_state.set(preview_state().close(tab_id));
                        },
                        onmove: move |request| {
                            preview_state.set(preview_state().move_tab(request));
                        },
                    }
                } else {
                    p { class: "mb-2 min-w-0 flex-1 px-2 text-ink-3", "No open diffs" }
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        class: "mb-1",
                        onclick: move |_| {
                            preview_state.set(PreviewTabState::new(tabs_reset.clone()));
                        },
                        "Reset"
                    }
                }
            }
            div {
                id: "viewer-active-view",
                class: "grid min-h-40 place-items-center px-6 py-10 text-center",
                div {
                    p { class: "text-xs text-ink-3", "Viewing" }
                    p { class: "mt-1 font-semibold text-ink", "{active_label}" }
                }
            }
            output { class: "sr-only", aria_live: "polite", "{state.announcement}" }
        }
    }
}

#[component]
fn ViewerTabRailDemo(tabs: Vec<ViewerTab>) -> Element {
    let mut preview_state = use_signal(move || PreviewTabState::new(tabs));
    let state = preview_state();
    let active_label = state
        .active_tab()
        .map_or_else(|| "No open diff".to_owned(), |tab| tab.label.clone());

    rsx! {
        section {
            class: "mx-auto w-full max-w-4xl overflow-hidden rounded-panel border border-line bg-bg shadow-floating",
            aria_label: "Interactive viewer tab rail",
            nav {
                class: "flex min-w-0 items-end border-b border-line bg-surface",
                aria_label: "Viewer navigation preview",
                ScrollArea {
                    variant: ScrollAreaVariant::Rail,
                    class: "flex min-w-0 flex-1 items-end gap-0 overflow-x-auto",
                    role: "tablist",
                    aria_label: "Open diffs",
                    for tab in &state.tabs {
                        {
                            let tab_id = tab.id;
                            let active = state.active_tab_id == Some(tab_id);
                            rsx! {
                                ViewerTabItem {
                                    key: "{tab.id}",
                                    tab: tab.clone(),
                                    active,
                                    rows_loading: tab.state == ViewerTabState::Pending,
                                    reorderable: true,
                                    onactivate: move |()| {
                                        preview_state.set(preview_state().activate(tab_id));
                                    },
                                    onkeydown: move |_| {},
                                    onclose: move |_| {
                                        preview_state.set(preview_state().close(tab_id));
                                    },
                                    onmove: move |request| {
                                        preview_state.set(preview_state().move_tab(request));
                                    },
                                }
                            }
                        }
                    }
                }
            }
            div {
                id: "viewer-active-view",
                class: "grid min-h-40 place-items-center px-6 py-10 text-center",
                div {
                    p { class: "text-xs text-ink-3", "Viewing" }
                    p { class: "mt-1 font-semibold text-ink", "{active_label}" }
                }
            }
            output { class: "sr-only", aria_live: "polite", "{state.announcement}" }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewTabState {
    tabs: Vec<ViewerTab>,
    active_tab_id: Option<ViewerTabId>,
    announcement: String,
}

impl PreviewTabState {
    fn new(tabs: Vec<ViewerTab>) -> Self {
        let active_tab_id = tabs.first().map(|tab| tab.id);
        Self {
            tabs,
            active_tab_id,
            announcement: String::new(),
        }
    }

    fn active_tab(&self) -> Option<&ViewerTab> {
        self.active_tab_id
            .and_then(|active_tab_id| self.tabs.iter().find(|tab| tab.id == active_tab_id))
    }

    fn activate(mut self, tab_id: ViewerTabId) -> Self {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == tab_id) else {
            return self;
        };
        self.active_tab_id = Some(tab_id);
        self.announcement = format!("Selected {}", tab.label);
        self
    }

    fn close(mut self, tab_id: ViewerTabId) -> Self {
        let Some(tab_index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return self;
        };
        let closed_tab = self.tabs.remove(tab_index);
        if self.active_tab_id == Some(tab_id) {
            self.active_tab_id = self
                .tabs
                .get(tab_index)
                .or_else(|| self.tabs.last())
                .map(|tab| tab.id);
        }
        self.announcement = format!("Closed {}", closed_tab.label);
        self
    }

    fn move_tab(mut self, request: MoveViewerTab) -> Self {
        let Some(from) = self.tabs.iter().position(|tab| tab.id == request.tab_id) else {
            return self;
        };
        let Some(target) = self
            .tabs
            .iter()
            .position(|tab| tab.id == request.target_tab_id)
        else {
            return self;
        };
        if from == target {
            return self;
        }
        let target_slot = match request.placement {
            gtl_models::viewer::ViewerTabPlacement::Before => target,
            gtl_models::viewer::ViewerTabPlacement::After => target + 1,
        };
        let insertion_index = if from < target_slot {
            target_slot - 1
        } else {
            target_slot
        };
        if from == insertion_index {
            return self;
        }

        let tab = self.tabs.remove(from);
        let label = tab.label.clone();
        self.tabs.insert(insertion_index, tab);
        self.announcement = format!("Moved {label} to position {}", insertion_index + 1);
        self
    }
}

fn preview_tabs_with_active() -> PreviewResult<(Vec<ViewerTab>, ViewerTab)> {
    let tabs = preview_tabs()?;
    let active_tab = tabs
        .first()
        .cloned()
        .ok_or_else(|| "viewer tab preview requires one tab".to_owned())?;
    Ok((tabs, active_tab))
}

fn preview_tabs() -> PreviewResult<Vec<ViewerTab>> {
    let fixtures = [
        ("Working tree", ViewerTabKind::Live, ViewerTabState::Ready),
        (
            "Feature branch",
            ViewerTabKind::Snapshot,
            ViewerTabState::Ready,
        ),
        (
            "Remote comparison",
            ViewerTabKind::Live,
            ViewerTabState::Pending,
        ),
        (
            "Refactor preview",
            ViewerTabKind::Snapshot,
            ViewerTabState::Ready,
        ),
        (
            "Release candidate",
            ViewerTabKind::Snapshot,
            ViewerTabState::Broken,
        ),
        (
            "Dependency update",
            ViewerTabKind::Live,
            ViewerTabState::Ready,
        ),
        (
            "Archived snapshot",
            ViewerTabKind::Snapshot,
            ViewerTabState::Error,
        ),
        (
            "Documentation edits",
            ViewerTabKind::Live,
            ViewerTabState::Ready,
        ),
    ];

    fixtures
        .into_iter()
        .enumerate()
        .map(|(index, (label, kind, state))| {
            Ok(ViewerTab {
                id: ViewerTabId::try_new(index as u64 + 1)?,
                label: label.to_owned(),
                kind,
                state,
            })
        })
        .collect()
}

fn preview_error(error: impl std::fmt::Display) -> Element {
    rsx! {
        p { role: "alert", "Preview unavailable: {error}" }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::ViewerTabPlacement;
    use gtl_wire::viewer::MoveViewerTab;

    use super::{PreviewResult, PreviewTabState, preview_tabs};

    #[test]
    fn closing_the_active_tab_selects_its_next_neighbor() -> PreviewResult<()> {
        let tabs = preview_tabs()?;
        let closing_tab_id = tabs[1].id;
        let expected_tab_id = tabs[2].id;
        let state = PreviewTabState::new(tabs)
            .activate(closing_tab_id)
            .close(closing_tab_id);

        assert_eq!(state.active_tab_id, Some(expected_tab_id));
        Ok(())
    }

    #[test]
    fn dragging_a_tab_reorders_the_preview_by_identity() -> PreviewResult<()> {
        let tabs = preview_tabs()?;
        let first = tabs[0].id;
        let third = tabs[2].id;
        let expected_order = [tabs[1].id, third, first, tabs[3].id];

        let state = PreviewTabState::new(tabs).move_tab(MoveViewerTab {
            tab_id: first,
            target_tab_id: third,
            placement: ViewerTabPlacement::After,
        });

        assert_eq!(
            state
                .tabs
                .iter()
                .take(expected_order.len())
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            expected_order
        );
        Ok(())
    }
}

/// Collapsed tab rail menu.
#[stories(
    id = "viewer-tab-overflow-menu",
    name = "Viewer tab overflow menu",
    thumbnail = thumbnail
)]
const VIEWER_TAB_OVERFLOW_MENU_STORIES: () = &[interactive, narrow_rail, tab_rail];
