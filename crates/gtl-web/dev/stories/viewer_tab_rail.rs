use std::error::Error;

use dioxus::prelude::*;
use dx_story::{stories, story};
use gtl_models::{
    paths::ProjectName, recipes::RecipeLabel, settings::ViewerLanguage, viewer::ViewerTabId,
};
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabState};

use crate::shared::{
    recipe_label::recipe_label_text,
    ui::{
        Button, ButtonSize, ButtonVariant, InlineTextSubmission, NavigationBar, ViewerTabItem,
        ViewerTabRail,
    },
};

type PreviewResult<T> = Result<T, Box<dyn Error>>;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "pointer-events-none", {demo(false)} }
    }
}

#[story]
fn interactive() -> Element {
    demo(false)
}

#[story(name = "Narrow rail")]
fn narrow_rail() -> Element {
    demo(true)
}

fn demo(narrow: bool) -> Element {
    let tabs = match preview_tabs() {
        Ok(tabs) => tabs,
        Err(error) => return preview_error(error),
    };
    rsx! {
        div { class: if narrow { "mx-auto w-80 max-w-full" } else { "w-full" },
            ViewerTabRailDemo { tabs }
        }
    }
}

#[component]
fn ViewerTabRailDemo(tabs: Vec<ViewerTab>) -> Element {
    let mut preview_state = use_signal(move || PreviewTabState::new(tabs));
    let state = preview_state();
    let active_label = state
        .active_tab()
        .map_or_else(|| "No open diff".to_owned(), label_text);

    rsx! {
        section {
            class: "story-viewer-frame mx-auto w-full",
            aria_label: "Interactive viewer tab rail",
            NavigationBar {
                aria_label: "Viewer navigation preview",
                rail: rsx! {
                    ViewerTabRail { active_tab_id: state.active_tab_id,
                        div { class: "flex w-max items-end",
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
                                            onpin: move |pinned| {
                                                preview_state.set(preview_state().pin(tab_id, pinned));
                                            },
                                            onrename: move |submission: InlineTextSubmission| {
                                                preview_state
                                                    .set(preview_state().rename(tab_id, &submission.value));
                                                (submission.complete)(Ok(()));
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
            div {
                id: "viewer-active-view",
                class: "story-active-view min-h-40 px-6 py-10",
                div {
                    p { class: "text-xs text-ink-3", "Viewing" }
                    p { class: "mt-1 font-semibold text-ink", "{active_label}" }
                }
            }
            output { class: "sr-only", aria_live: "polite", "{state.announcement}" }
            div { class: "flex flex-wrap gap-2 border-t border-line p-3",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| {
                        if let Some(tab) = preview_state().tabs.first() {
                            preview_state.set(preview_state().activate(tab.id));
                        }
                    },
                    "Review first diff"
                }
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| {
                        if let Some(tab) = preview_state().tabs.last() {
                            preview_state.set(preview_state().activate(tab.id));
                        }
                    },
                    "Review last diff"
                }
            }
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
        self.announcement = format!("Selected {}", label_text(tab));
        self
    }

    fn close(mut self, tab_id: ViewerTabId) -> Self {
        let Some(tab_index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return self;
        };
        let closed_tab = self.tabs.remove(tab_index);
        if self.active_tab_id == Some(tab_id) {
            self.active_tab_id = self.tabs.first().map(|tab| tab.id);
        }
        self.announcement = format!("Closed {}", label_text(&closed_tab));
        self
    }

    /// Pins or unpins a tab; pinned tabs lead the rail, as in the viewer.
    fn pin(mut self, tab_id: ViewerTabId, pinned: bool) -> Self {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return self;
        };
        tab.pinned = pinned;
        self.announcement = format!(
            "{} {}",
            if pinned { "Pinned" } else { "Unpinned" },
            label_text(tab)
        );
        self.tabs.sort_by_key(|tab| !tab.pinned);
        self
    }

    fn rename(mut self, tab_id: ViewerTabId, name: &str) -> Self {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id) else {
            return self;
        };
        let Ok(name) = ProjectName::try_new(name.trim()) else {
            return self;
        };
        tab.custom_name = Some(name.to_string());
        self.announcement = format!("Renamed to {name}");
        tab.label = RecipeLabel::Named { name };
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
        let label = label_text(&tab);
        self.tabs.insert(insertion_index, tab);
        self.announcement = format!("Moved {label} to position {}", insertion_index + 1);
        self
    }
}

fn preview_tabs() -> PreviewResult<Vec<ViewerTab>> {
    let fixtures = [
        ("Working tree", true, ViewerTabState::Ready),
        ("Feature branch", false, ViewerTabState::Ready),
        ("Remote comparison", true, ViewerTabState::Pending),
        ("Refactor preview", false, ViewerTabState::Ready),
        ("Release candidate", false, ViewerTabState::Broken),
        ("Dependency update", true, ViewerTabState::Ready),
        ("Archived snapshot", false, ViewerTabState::Error),
        ("Documentation edits", true, ViewerTabState::Ready),
    ];

    fixtures
        .into_iter()
        .enumerate()
        .map(|(index, (label, live, state))| {
            Ok(ViewerTab {
                details: None,
                custom_name: None,
                pinned: false,
                id: ViewerTabId::try_new(index as u64 + 1)?,
                label: RecipeLabel::Named {
                    name: ProjectName::try_new(label)?,
                },
                live,
                state,
            })
        })
        .collect()
}

fn label_text(tab: &ViewerTab) -> String {
    recipe_label_text(&tab.label, ViewerLanguage::EnUs)
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
    fn closing_the_active_tab_selects_the_first_remaining() -> PreviewResult<()> {
        let tabs = preview_tabs()?;
        let closing_tab_id = tabs[1].id;
        let expected_tab_id = tabs[0].id;
        let state = PreviewTabState::new(tabs)
            .activate(closing_tab_id)
            .close(closing_tab_id);

        assert_eq!(state.active_tab_id, Some(expected_tab_id));
        Ok(())
    }

    #[test]
    fn pinning_moves_the_tab_ahead_of_unpinned_tabs() -> PreviewResult<()> {
        let tabs = preview_tabs()?;
        let third = tabs[2].id;
        let expected_order = [third, tabs[0].id, tabs[1].id];

        let state = PreviewTabState::new(tabs).pin(third, true);

        assert_eq!(
            state
                .tabs
                .iter()
                .take(expected_order.len())
                .map(|tab| tab.id)
                .collect::<Vec<_>>(),
            expected_order
        );
        assert!(state.tabs[0].pinned);
        Ok(())
    }

    #[test]
    fn renaming_trims_the_name_and_ignores_blank_drafts() -> PreviewResult<()> {
        let tabs = preview_tabs()?;
        let snapshot = tabs[1].id;

        let state = PreviewTabState::new(tabs)
            .rename(snapshot, "  Review notes  ")
            .rename(snapshot, "   ");

        assert_eq!(super::label_text(&state.tabs[1]), "Review notes");
        assert_eq!(state.tabs[1].custom_name.as_deref(), Some("Review notes"));
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

/// Horizontally scrolling diff tabs.
#[stories(
    id = "viewer-tab-rail",
    name = "Viewer tab rail",
    thumbnail = thumbnail
)]
const VIEWER_TAB_RAIL_STORIES: () = &[interactive, narrow_rail];
