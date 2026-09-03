use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::{ViewerTab, ViewerTabKind, ViewerTabState};
#[cfg(feature = "component-preview")]
use lucide_dioxus::{Check, ChevronDown};
use lucide_dioxus::{TriangleAlert, X};

use super::{Button, ButtonSize, ButtonVariant, LoadingSpinner};
#[cfg(feature = "component-preview")]
use super::{CountBadge, ScrollArea};

#[cfg(feature = "component-preview")]
const VIEWER_TAB_OVERFLOW_PANEL_CLASSES: &str = "fixed inset-auto z-70 m-0 mt-1 mb-2 grid max-h-[min(28rem,calc(100%-0.75rem))] w-[min(26rem,calc(100vw-1rem))] origin-top-right grid-rows-[auto_minmax(0,1fr)] overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-floating [position-area:bottom_span-left] open:animate-popover-enter motion-reduce:animate-none";

#[component]
pub(crate) fn ViewerTabItem(
    tab: ViewerTab,
    active: bool,
    #[props(default)] rows_loading: bool,
    onactivate: EventHandler<MouseEvent>,
    onkeydown: EventHandler<KeyboardEvent>,
    onclose: EventHandler<MouseEvent>,
) -> Element {
    let presentation_state = tab_presentation_state(&tab.state, rows_loading);
    let tab_id = tab.id;

    rsx! {
        div { class: if active { "group/viewer-tab flex min-w-24 max-w-56 shrink-0 items-center border-b-2 border-acc bg-surface-2 text-ink" } else { "group/viewer-tab flex min-w-24 max-w-56 shrink-0 items-center border-b-2 border-transparent bg-transparent text-ink-2 hover:bg-surface-2 hover:text-ink" },
            button {
                id: viewer_tab_element_id(tab_id),
                class: "flex min-w-0 flex-1 cursor-pointer items-center gap-2 border-0 bg-transparent py-1.5 pr-1 pl-2 text-left text-inherit focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc",
                r#type: "button",
                role: "tab",
                aria_selected: active.to_string(),
                aria_busy: presentation_state.is_loading().to_string(),
                aria_controls: "viewer-active-view",
                tabindex: if active { "0" } else { "-1" },
                title: tab.label.clone(),
                onclick: onactivate,
                onkeydown,
                span {
                    class: "inline-flex size-4 flex-none items-center justify-center rounded-sm border border-line-2 text-xs leading-none font-bold text-ink-3",
                    aria_hidden: "true",
                    {tab_kind_label(tab.kind)}
                }
                span { class: "flex min-w-0 flex-1 items-center gap-1.5",
                    span { class: "min-w-0 flex-1 truncate", "{tab.label}" }
                    if tab.kind == ViewerTabKind::Live {
                        span { class: "sr-only", ", Live" }
                    }
                    TabStateMarker { state: presentation_state }
                }
            }
            ViewerTabCloseButton {
                label: tab.label.clone(),
                test_id: gtl_web_contracts::test_ids::VIEWER_TAB_CLOSE.value().to_owned(),
                onclick: onclose,
            }
        }
    }
}

#[component]
fn ViewerTabCloseButton(
    label: String,
    #[props(default)] test_id: Option<String>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        Button {
            size: ButtonSize::IconCompact,
            variant: ButtonVariant::Bare,
            class: "group/viewer-tab-close relative mr-1 flex-none text-ink-3 hover:text-bg focus-visible:text-bg",
            aria_label: "Close {label}",
            title: "Close tab",
            "data-testid": test_id,
            onclick,
            span {
                class: "absolute inset-0 m-auto size-5 rounded-full bg-del opacity-0 transition-opacity duration-150 ease-out group-hover/viewer-tab-close:opacity-100 group-focus-visible/viewer-tab-close:opacity-100 motion-reduce:transition-none",
                aria_hidden: "true",
            }
            span { class: "relative z-2", aria_hidden: "true",
                X { size: 12 }
            }
        }
    }
}

#[cfg(feature = "component-preview")]
#[component]
pub(crate) fn ViewerTabOverflowMenu(
    id: String,
    tabs: Vec<ViewerTab>,
    active_tab: ViewerTab,
    #[props(default)] diff_rows_loading_tab_id: Option<ViewerTabId>,
    onactivate: EventHandler<ViewerTabId>,
    onclose: EventHandler<ViewerTabId>,
) -> Element {
    let active_tab_state = tab_presentation_state(
        &active_tab.state,
        diff_rows_loading_tab_id == Some(active_tab.id),
    );
    let trigger_id = format!("{id}-trigger");
    let title_id = format!("{id}-title");
    let trigger_label = format!(
        "Choose open diff. Current: {}. {} open diffs.",
        active_tab.label,
        tabs.len()
    );

    rsx! {
        div { class: "group/viewer-tab-overflow flex min-w-0 flex-1 items-end",
            button {
                id: trigger_id,
                class: "flex h-9 w-full min-w-0 max-w-[30rem] cursor-pointer items-center gap-2 border-0 border-b-2 border-acc bg-surface-2 px-2.5 text-left text-ink hover:bg-line focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc group-has-[:popover-open]/viewer-tab-overflow:bg-line",
                r#type: "button",
                popovertarget: id.clone(),
                popovertargetaction: "toggle",
                aria_label: trigger_label.clone(),
                aria_haspopup: "dialog",
                aria_controls: id.clone(),
                title: trigger_label,
                span {
                    class: "inline-flex size-4 flex-none items-center justify-center rounded-sm border border-line-2 text-xs leading-none font-bold text-ink-3",
                    aria_hidden: "true",
                    {tab_kind_label(active_tab.kind)}
                }
                span { class: "min-w-0 flex-1 truncate font-medium", "{active_tab.label}" }
                TabStateMarker { state: active_tab_state }
                CountBadge { count: tabs.len(), aria_hidden: "true" }
                span {
                    class: "flex-none text-ink-3 transition-transform duration-150 ease-out group-has-[:popover-open]/viewer-tab-overflow:rotate-180 motion-reduce:transition-none",
                    aria_hidden: "true",
                    ChevronDown { size: 15 }
                }
            }
            div {
                id,
                class: VIEWER_TAB_OVERFLOW_PANEL_CLASSES,
                popover: "auto",
                role: "dialog",
                aria_labelledby: title_id.clone(),
                header { class: "flex items-center gap-3 border-b border-line px-3 py-2.5",
                    div { class: "min-w-0 flex-1",
                        h2 {
                            id: title_id,
                            class: "text-sm font-semibold text-ink",
                            "Open diffs"
                        }
                        p { class: "text-xs text-ink-3", "Select a diff or close one" }
                    }
                    CountBadge {
                        count: tabs.len(),
                        aria_label: "{tabs.len()} open diffs",
                    }
                }
                ScrollArea { class: "min-h-0 overscroll-contain overflow-y-auto p-1.5",
                    ul { class: "grid gap-px", role: "list",
                        for tab in &tabs {
                            {
                                let tab_id = tab.id;
                                let active = active_tab.id == tab_id;
                                let presentation_state = tab_presentation_state(
                                    &tab.state,
                                    diff_rows_loading_tab_id == Some(tab_id),
                                );
                                rsx! {
                                    li { class: if active { "group/viewer-tab-menu flex min-w-0 items-center rounded-sm bg-surface-2 text-ink" } else { "group/viewer-tab-menu flex min-w-0 items-center rounded-sm bg-transparent text-ink-2 hover:bg-surface-2 hover:text-ink" },
                                        button {
                                            class: "flex min-h-11 min-w-0 flex-1 cursor-pointer items-center gap-2 border-0 bg-transparent px-2 py-1 text-left text-inherit focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc",
                                            r#type: "button",
                                            popovertarget: id.clone(),
                                            popovertargetaction: "hide",
                                            aria_current: active.then_some("page"),
                                            aria_controls: "viewer-active-view",
                                            onclick: move |_| onactivate.call(tab_id),
                                            span {
                                                class: "inline-flex size-7 flex-none items-center justify-center rounded-sm border border-line bg-sunk text-xs font-bold text-ink-3",
                                                aria_hidden: "true",
                                                {tab_kind_label(tab.kind)}
                                            }
                                            span { class: "min-w-0 flex-1",
                                                strong { class: "block truncate text-xs font-semibold text-inherit", "{tab.label}" }
                                                small { class: "mt-0.5 block truncate text-xs text-ink-3", {tab_kind_name(tab.kind)} }
                                            }
                                            span { class: "flex flex-none items-center gap-1.5",
                                                TabStateMarker { state: presentation_state }
                                                if let Some(label) = presentation_state.menu_label() {
                                                    small { class: "text-xs text-ink-3", aria_hidden: "true", "{label}" }
                                                }
                                                if active {
                                                    span {
                                                        class: "inline-flex size-4 items-center justify-center text-acc",
                                                        aria_hidden: "true",
                                                        Check { size: 15 }
                                                    }
                                                }
                                            }
                                        }
                                        ViewerTabCloseButton { label: tab.label.clone(), onclick: move |_| onclose.call(tab_id) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn viewer_tab_element_id(tab_id: ViewerTabId) -> String {
    format!("viewer-tab-{tab_id}")
}

const fn tab_kind_label(kind: ViewerTabKind) -> &'static str {
    match kind {
        ViewerTabKind::Snapshot => "S",
        ViewerTabKind::Live => "L",
    }
}

#[cfg(feature = "component-preview")]
const fn tab_kind_name(kind: ViewerTabKind) -> &'static str {
    match kind {
        ViewerTabKind::Snapshot => "Snapshot",
        ViewerTabKind::Live => "Live view",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabPresentationState {
    Ready,
    Loading,
    Broken,
    Error,
}

impl TabPresentationState {
    const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Loading => "Rendering",
            Self::Broken => "Render stopped",
            Self::Error => "Render failed",
        }
    }

    const fn is_loading(self) -> bool {
        matches!(self, Self::Loading)
    }

    #[cfg(feature = "component-preview")]
    const fn menu_label(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Loading | Self::Broken | Self::Error => Some(self.label()),
        }
    }
}

const fn tab_presentation_state(
    state: &ViewerTabState,
    diff_rows_loading: bool,
) -> TabPresentationState {
    match state {
        ViewerTabState::Ready if diff_rows_loading => TabPresentationState::Loading,
        ViewerTabState::Ready => TabPresentationState::Ready,
        ViewerTabState::Pending => TabPresentationState::Loading,
        ViewerTabState::Broken => TabPresentationState::Broken,
        ViewerTabState::Error => TabPresentationState::Error,
    }
}

#[component]
fn TabStateMarker(state: TabPresentationState) -> Element {
    let label = state.label();

    rsx! {
        span {
            class: "inline-flex size-3.5 flex-none items-center justify-center text-acc",
            aria_hidden: "true",
            match state {
                TabPresentationState::Ready => rsx! {},
                TabPresentationState::Loading => rsx! {
                    LoadingSpinner {}
                },
                TabPresentationState::Broken | TabPresentationState::Error => rsx! {
                    span { class: "text-del",
                        TriangleAlert { size: 13 }
                    }
                },
            }
        }
        span { class: "sr-only", ", {label}" }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use gtl_wire::viewer::{ViewerTab, ViewerTabKind, ViewerTabState};

    use super::{
        TabPresentationState, TabStateMarker, ViewerTabItem, ViewerTabItemProps,
        tab_presentation_state,
    };
    #[cfg(feature = "component-preview")]
    use super::{
        VIEWER_TAB_OVERFLOW_PANEL_CLASSES, ViewerTabOverflowMenu, ViewerTabOverflowMenuProps,
    };
    use crate::test_support::{TestResult, viewer_tab_id};

    #[test]
    fn row_stream_loading_uses_the_tab_loading_state() {
        assert_eq!(
            tab_presentation_state(&ViewerTabState::Ready, true),
            TabPresentationState::Loading
        );
        assert_eq!(
            tab_presentation_state(&ViewerTabState::Ready, false),
            TabPresentationState::Ready
        );
        assert_eq!(
            tab_presentation_state(&ViewerTabState::Error, true),
            TabPresentationState::Error
        );
    }

    #[test]
    fn every_tab_state_has_a_non_color_label() {
        assert_eq!(TabPresentationState::Ready.label(), "Ready");
        assert_eq!(TabPresentationState::Loading.label(), "Rendering");
        assert_eq!(TabPresentationState::Broken.label(), "Render stopped");
        assert_eq!(TabPresentationState::Error.label(), "Render failed");
    }

    #[test]
    fn tab_state_marker_reserves_its_footprint_while_idle() {
        for state in [
            TabPresentationState::Ready,
            TabPresentationState::Loading,
            TabPresentationState::Broken,
            TabPresentationState::Error,
        ] {
            let html = dioxus_ssr::render_element(rsx! {
                TabStateMarker { state }
            });

            assert!(html.contains("inline-flex size-3.5 flex-none"));
        }
    }

    #[test]
    fn active_tab_surface_wraps_the_activation_and_close_controls() -> TestResult {
        let tab_id = viewer_tab_id(1)?;
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ViewerTabItemProps {
            tab: ViewerTab {
                id: tab_id,
                label: "Working tree".to_owned(),
                kind: ViewerTabKind::Live,
                state: ViewerTabState::Ready,
            },
            active: true,
            rows_loading: false,
            onactivate: EventHandler::new(|_| {}),
            onkeydown: EventHandler::new(|_| {}),
            onclose: EventHandler::new(|_| {}),
        });
        let mut tab = VirtualDom::new_with_props(ViewerTabItem, props);
        tab.rebuild_in_place();
        let html = dioxus_ssr::render(&tab);

        assert!(html.starts_with("<div class=\"group/viewer-tab"));
        assert!(html.contains("border-acc bg-surface-2"));
        assert!(!html.contains("active:bg-"));
        assert!(html.contains("group-hover/viewer-tab-close:opacity-100"));
        Ok(())
    }

    #[cfg(feature = "component-preview")]
    #[test]
    fn overflow_panel_stays_below_its_trigger_with_bounded_height() {
        assert!(VIEWER_TAB_OVERFLOW_PANEL_CLASSES.contains("[position-area:bottom_span-left]"));
        assert!(
            VIEWER_TAB_OVERFLOW_PANEL_CLASSES.contains("max-h-[min(28rem,calc(100%-0.75rem))]")
        );
        assert!(!VIEWER_TAB_OVERFLOW_PANEL_CLASSES.contains("position-try-fallbacks"));
    }

    #[cfg(feature = "component-preview")]
    #[test]
    fn overflow_menu_names_the_current_tab_and_exposes_every_close_action() -> TestResult {
        let active_tab = ViewerTab {
            id: viewer_tab_id(1)?,
            label: "Working tree".to_owned(),
            kind: ViewerTabKind::Live,
            state: ViewerTabState::Ready,
        };
        let pending_tab = ViewerTab {
            id: viewer_tab_id(2)?,
            label: "Saved comparison".to_owned(),
            kind: ViewerTabKind::Snapshot,
            state: ViewerTabState::Pending,
        };
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ViewerTabOverflowMenuProps {
            id: "viewer-tab-overflow-test".to_owned(),
            tabs: vec![active_tab.clone(), pending_tab],
            active_tab,
            diff_rows_loading_tab_id: None,
            onactivate: EventHandler::new(|_| {}),
            onclose: EventHandler::new(|_| {}),
        });
        let mut menu = VirtualDom::new_with_props(ViewerTabOverflowMenu, props);
        menu.rebuild_in_place();
        let html = dioxus_ssr::render(&menu);

        assert!(html.contains("Choose open diff. Current: Working tree. 2 open diffs."));
        assert!(html.contains("aria-current=\"page\""));
        assert!(html.contains("Close Working tree"));
        assert!(html.contains("Close Saved comparison"));
        assert!(html.contains("Rendering"));
        Ok(())
    }
}
