use dioxus::{html::input_data::MouseButton, prelude::*};
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabKind, ViewerTabState};
use lucide_dioxus::{ArrowUp, FileDiff, Radio, TriangleAlert, X};
#[cfg(any(feature = "component-preview", feature = "desktop"))]
use lucide_dioxus::{Check, ChevronDown};

use super::{Button, ButtonSize, ButtonVariant, LoadingSpinner};
#[cfg(any(feature = "component-preview", feature = "desktop"))]
use super::{CountBadge, ScrollArea};

#[cfg(any(feature = "component-preview", feature = "desktop"))]
const VIEWER_TAB_OVERFLOW_PANEL_CLASSES: &str = "fixed inset-auto z-70 m-0 mt-1 mb-2 max-h-[min(28rem,calc(100%-0.75rem))] w-[min(26rem,calc(100vw-1rem))] origin-top-right grid-rows-[auto_minmax(0,1fr)] overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-floating [position-area:bottom_span-left] open:grid open:animate-popover-enter motion-reduce:animate-none";

mod pointer_drag;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ViewerTabActivationGesture {
    suppress_next_click: bool,
}

impl ViewerTabActivationGesture {
    fn pointer_down(&mut self, trigger_button: Option<MouseButton>) -> bool {
        if trigger_button != Some(MouseButton::Primary) {
            return false;
        }
        self.suppress_next_click = true;
        true
    }

    fn click(&mut self) -> bool {
        !std::mem::take(&mut self.suppress_next_click)
    }

    fn cancel(&mut self) {
        self.suppress_next_click = false;
    }
}

#[component]
pub(crate) fn ViewerTabItem(
    tab: ViewerTab,
    active: bool,
    #[props(default)] rows_loading: bool,
    #[props(default)] reorderable: bool,
    onactivate: EventHandler<()>,
    onkeydown: EventHandler<KeyboardEvent>,
    onclose: EventHandler<MouseEvent>,
    #[props(default)] onmove: Option<EventHandler<MoveViewerTab>>,
) -> Element {
    let presentation_state = tab_presentation_state(&tab.state, rows_loading);
    let tab_id = tab.id;
    let mut activation_gesture = use_signal(ViewerTabActivationGesture::default);
    let drag = pointer_drag::use_pointer_drag(onmove);
    let surface_classes = if active {
        "bg-surface-2 text-ink"
    } else {
        "bg-sunk text-ink-2 hover:bg-surface-2 hover:text-ink"
    };
    let activation_classes = if reorderable {
        "touch-none cursor-default"
    } else {
        "cursor-default"
    };

    rsx! {
        div {
            class: "group/viewer-tab relative flex h-9 min-w-24 max-w-72 shrink-0 select-none items-center {surface_classes} duration-[160ms] ease-out data-[drag-state=shifting]:transition-transform motion-reduce:transition-none data-[drag-state=dragging]:opacity-0 data-[drag-state=shifting]:will-change-transform",
            "data-viewer-tab-id": "{tab_id}",
            "data-viewer-tab-axis": "horizontal",
            button {
                id: viewer_tab_element_id(tab_id),
                class: "flex h-full min-w-0 flex-1 items-center gap-1.5 border-0 bg-transparent pr-1 pl-2 text-left text-inherit focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc {activation_classes}",
                r#type: "button",
                draggable: "false",
                role: "tab",
                aria_roledescription: reorderable.then_some("sortable tab"),
                aria_selected: active.to_string(),
                aria_busy: presentation_state.is_loading().to_string(),
                aria_controls: "viewer-active-view",
                tabindex: if active { "0" } else { "-1" },
                title: tab_description(&tab),
                onpointerdown: move |event: PointerEvent| {
                    if reorderable {
                        drag.start.call(event.clone());
                    }
                    if activation_gesture
                        .write()
                        .pointer_down(event.trigger_button())
                    {
                        onactivate.call(());
                    }
                },
                onpointermove: move |event| drag.move_pointer.call(event),
                onpointerup: move |event| drag.release.call(event),
                onlostpointercapture: move |_| drag.cancel.call(()),
                onclick: move |event| {
                    if drag.suppress_click.call(()) {
                        event.prevent_default();
                        return;
                    }
                    if activation_gesture.write().click() {
                        onactivate.call(());
                    }
                },
                onkeydown: move |event| {
                    activation_gesture.write().cancel();
                    if event.key() == Key::Escape {
                        drag.cancel.call(());
                    }
                    onkeydown.call(event);
                },
                onblur: move |_| activation_gesture.write().cancel(),
                onpointercancel: move |_| {
                    activation_gesture.write().cancel();
                    drag.cancel.call(());
                },
                {viewer_tab_rail_content(&tab, presentation_state)}
            }
            ViewerTabCloseButton {
                label: tab.label.clone(),
                test_id: gtl_web_contracts::test_ids::VIEWER_TAB_CLOSE.value().to_owned(),
                onclick: onclose,
            }
            span {
                class: if active { "pointer-events-none absolute inset-x-0 bottom-0 h-0.5 bg-acc opacity-100 transition-opacity ease-out motion-reduce:transition-none" } else { "pointer-events-none absolute inset-x-0 bottom-0 h-0.5 bg-acc opacity-0" },
                style: active.then_some("transition-duration:75ms;"),
                "data-viewer-tab-selection-indicator": "true",
                aria_hidden: "true",
            }
        }
    }
}

#[component]
pub(crate) fn ViewerTabRailMeasurementItem(
    tab: ViewerTab,
    #[props(default)] rows_loading: bool,
) -> Element {
    let presentation_state = tab_presentation_state(&tab.state, rows_loading);

    rsx! {
        div {
            class: "flex h-9 min-w-24 max-w-72 shrink-0 select-none items-center",
            "data-viewer-tab-measurement": "true",
            aria_hidden: "true",
            span { class: "flex h-full min-w-0 flex-1 items-center gap-1.5 pr-1 pl-2",
                {viewer_tab_rail_content(&tab, presentation_state)}
            }
            span { class: "mr-1 size-6 flex-none", aria_hidden: "true" }
        }
    }
}

fn tab_description(tab: &ViewerTab) -> String {
    match tab.kind {
        ViewerTabKind::LiveLocalChanges => format!("{} - Local changes", tab.label),
        ViewerTabKind::LiveUnpushedCommits => format!("{} - Unpushed commits", tab.label),
        ViewerTabKind::Snapshot | ViewerTabKind::Live => tab.label.clone(),
    }
}

fn viewer_tab_rail_content(tab: &ViewerTab, presentation_state: TabPresentationState) -> Element {
    rsx! {
        if presentation_state == TabPresentationState::Ready {
            ViewerTabKindIndicator { kind: tab.kind }
        } else {
            TabStateMarker { state: presentation_state }
        }
        span { class: "min-w-0 truncate", "{tab.label}" }
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
            class: "group/viewer-tab-close relative mr-1 flex-none text-white",
            aria_label: "Close {label}",
            "data-testid": test_id,
            onclick,
            span {
                class: "pointer-events-none absolute inset-0 m-auto size-5 rounded-full opacity-0 transition-opacity ease-out group-hover/viewer-tab-close:opacity-100 group-focus-visible/viewer-tab-close:opacity-100 motion-reduce:transition-none",
                style: "background-color:#c1121f;transition-duration:100ms;",
                aria_hidden: "true",
            }
            span {
                class: "pointer-events-none absolute inset-0 z-2 grid place-items-center text-white",
                style: "transform:translateX(-0.5px);",
                aria_hidden: "true",
                X { size: 14, stroke_width: 4 }
            }
        }
    }
}

#[component]
fn ViewerTabKindIndicator(kind: ViewerTabKind) -> Element {
    let label = match kind {
        ViewerTabKind::Snapshot => "",
        ViewerTabKind::Live => ", Live",
        ViewerTabKind::LiveLocalChanges => ", Live, Local changes",
        ViewerTabKind::LiveUnpushedCommits => ", Live, Unpushed commits",
    };
    rsx! {
        span {
            class: "inline-flex size-3.5 flex-none items-center justify-center text-add",
            aria_hidden: "true",
            match kind {
                ViewerTabKind::Snapshot => rsx! {},
                ViewerTabKind::Live => rsx! {
                    Radio { size: 13 }
                },
                ViewerTabKind::LiveLocalChanges => rsx! {
                    FileDiff { size: 13 }
                },
                ViewerTabKind::LiveUnpushedCommits => rsx! {
                    ArrowUp { size: 13 }
                },
            }
        }
        if !label.is_empty() {
            span { class: "sr-only", "{label}" }
        }
    }
}

#[cfg(any(feature = "component-preview", feature = "desktop"))]
#[component]
pub(crate) fn ViewerTabOverflowMenu(
    id: String,
    tabs: Vec<ViewerTab>,
    active_tab: ViewerTab,
    #[props(default)] diff_rows_loading_tab_id: Option<ViewerTabId>,
    #[props(default = true)] reorderable: bool,
    onactivate: EventHandler<ViewerTabId>,
    onclose: EventHandler<ViewerTabId>,
    onmove: EventHandler<MoveViewerTab>,
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
                class: "relative flex h-9 w-full min-w-0 max-w-[30rem] cursor-default select-none items-center gap-1.5 border-0 bg-surface-2 px-2.5 text-left text-ink hover:bg-line focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc group-has-[:popover-open]/viewer-tab-overflow:bg-line",
                r#type: "button",
                popovertarget: id.clone(),
                popovertargetaction: "toggle",
                aria_label: trigger_label.clone(),
                aria_haspopup: "dialog",
                aria_controls: id.clone(),
                title: trigger_label,
                "data-testid": gtl_web_contracts::test_ids::VIEWER_TAB_OVERFLOW_TRIGGER.value(),
                ViewerTabKindIndicator { kind: active_tab.kind }
                span { class: "min-w-0 flex-1 truncate font-medium", "{active_tab.label}" }
                TabStateMarker { state: active_tab_state }
                CountBadge { count: tabs.len(), aria_hidden: "true" }
                span {
                    class: "flex-none text-ink-3 transition-transform ease-out group-has-[:popover-open]/viewer-tab-overflow:rotate-180 motion-reduce:transition-none",
                    style: "transition-duration:100ms;",
                    aria_hidden: "true",
                    ChevronDown { size: 15 }
                }
                span {
                    class: "pointer-events-none absolute inset-x-0 bottom-0 h-0.5 bg-acc opacity-100",
                    aria_hidden: "true",
                }
            }
            div {
                id,
                class: VIEWER_TAB_OVERFLOW_PANEL_CLASSES,
                style: "height: fit-content;",
                popover: "auto",
                role: "dialog",
                aria_labelledby: title_id.clone(),
                "data-testid": gtl_web_contracts::test_ids::VIEWER_TAB_OVERFLOW_MENU.value(),
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
                                    ViewerTabOverflowMenuItem {
                                        key: "{tab.id}",
                                        popover_id: id.clone(),
                                        tab: tab.clone(),
                                        active,
                                        presentation_state,
                                        reorderable,
                                        onactivate,
                                        onclose,
                                        onmove,
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

#[cfg(any(feature = "component-preview", feature = "desktop"))]
#[component]
fn ViewerTabOverflowMenuItem(
    popover_id: String,
    tab: ViewerTab,
    active: bool,
    presentation_state: TabPresentationState,
    reorderable: bool,
    onactivate: EventHandler<ViewerTabId>,
    onclose: EventHandler<ViewerTabId>,
    onmove: EventHandler<MoveViewerTab>,
) -> Element {
    let tab_id = tab.id;
    let mut activation_gesture = use_signal(ViewerTabActivationGesture::default);
    let drag = pointer_drag::use_pointer_drag(Some(onmove));
    let surface_classes = if active {
        "bg-surface-2 text-ink"
    } else {
        "bg-transparent text-ink-2 hover:bg-surface-2 hover:text-ink"
    };
    let activation_classes = if reorderable {
        "touch-none cursor-default"
    } else {
        "cursor-default"
    };

    rsx! {
        li {
            class: "group/viewer-tab-menu relative flex min-w-0 select-none items-center rounded-sm {surface_classes} duration-[160ms] ease-out data-[drag-state=shifting]:transition-transform motion-reduce:transition-none data-[drag-state=dragging]:opacity-0 data-[drag-state=shifting]:will-change-transform",
            "data-viewer-tab-id": "{tab_id}",
            "data-viewer-tab-axis": "vertical",
            button {
                class: "flex min-h-10 min-w-0 flex-1 items-center gap-1.5 border-0 bg-transparent px-2 py-1 text-left text-inherit focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc {activation_classes}",
                r#type: "button",
                draggable: "false",
                popovertarget: popover_id,
                popovertargetaction: "hide",
                aria_roledescription: reorderable.then_some("sortable tab"),
                aria_current: active.then_some("page"),
                aria_controls: "viewer-active-view",
                onpointerdown: move |event: PointerEvent| {
                    if reorderable {
                        drag.start.call(event.clone());
                    }
                    if activation_gesture
                        .write()
                        .pointer_down(event.trigger_button())
                    {
                        onactivate.call(tab_id);
                    }
                },
                onpointermove: move |event| drag.move_pointer.call(event),
                onpointerup: move |event| drag.release.call(event),
                onlostpointercapture: move |_| drag.cancel.call(()),
                onclick: move |event| {
                    if drag.suppress_click.call(()) {
                        event.prevent_default();
                        return;
                    }
                    if activation_gesture.write().click() {
                        onactivate.call(tab_id);
                    }
                },
                onkeydown: move |event| {
                    activation_gesture.write().cancel();
                    if event.key() == Key::Escape {
                        event.prevent_default();
                        drag.cancel.call(());
                    }
                },
                onblur: move |_| activation_gesture.write().cancel(),
                onpointercancel: move |_| {
                    activation_gesture.write().cancel();
                    drag.cancel.call(());
                },
                ViewerTabKindIndicator { kind: tab.kind }
                strong { class: "min-w-0 flex-1 truncate text-xs font-semibold text-inherit",
                    "{tab.label}"
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
            ViewerTabCloseButton {
                label: tab.label.clone(),
                onclick: move |_| onclose.call(tab_id),
            }
        }
    }
}

pub(crate) fn viewer_tab_element_id(tab_id: ViewerTabId) -> String {
    format!("viewer-tab-{tab_id}")
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

    #[cfg(any(feature = "component-preview", feature = "desktop"))]
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
        ViewerTabState::Ready | ViewerTabState::Pending if diff_rows_loading => {
            TabPresentationState::Loading
        }
        ViewerTabState::Ready | ViewerTabState::Pending => TabPresentationState::Ready,
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
        MouseButton, TabPresentationState, TabStateMarker, ViewerTabActivationGesture,
        ViewerTabItem, ViewerTabItemProps, ViewerTabRailMeasurementItem, tab_presentation_state,
    };
    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    use super::{
        VIEWER_TAB_OVERFLOW_PANEL_CLASSES, ViewerTabOverflowMenu, ViewerTabOverflowMenuProps,
    };
    use crate::test_support::{TestResult, viewer_tab_id};

    #[test]
    fn row_stream_loading_uses_the_tab_loading_state() {
        for state in [ViewerTabState::Ready, ViewerTabState::Pending] {
            assert_eq!(
                tab_presentation_state(&state, true),
                TabPresentationState::Loading
            );
            assert_eq!(
                tab_presentation_state(&state, false),
                TabPresentationState::Ready
            );
        }
        for rows_loading in [false, true] {
            assert_eq!(
                tab_presentation_state(&ViewerTabState::Error, rows_loading),
                TabPresentationState::Error
            );
            assert_eq!(
                tab_presentation_state(&ViewerTabState::Broken, rows_loading),
                TabPresentationState::Broken
            );
        }
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
            reorderable: false,
            onactivate: EventHandler::new(|()| {}),
            onkeydown: EventHandler::new(|_| {}),
            onclose: EventHandler::new(|_| {}),
            onmove: None,
        });
        let mut tab = VirtualDom::new_with_props(ViewerTabItem, props);
        tab.rebuild_in_place();
        let html = dioxus_ssr::render(&tab);

        assert!(html.starts_with("<div class=\"group/viewer-tab"));
        assert!(html.contains("bg-surface-2 text-ink"));
        assert!(html.contains("bg-acc opacity-100 transition-opacity ease-out"));
        assert!(html.contains("style=\"transition-duration:75ms;\""));
        assert!(!html.contains("duration-150"));
        assert!(!html.contains("active:bg-"));
        assert!(html.contains("group-hover/viewer-tab-close:opacity-100"));
        assert!(html.contains("background-color:#c1121f;transition-duration:100ms"));
        assert!(html.contains("text-white"));
        assert!(html.contains("transform:translateX(-0.5px)"));
        assert!(html.contains("stroke-width=\"4\""));
        assert!(html.contains("aria-label=\"Close Working tree\""));
        assert!(!html.contains("title=\"Close tab\""));
        assert!(html.contains(", Live"));
        assert!(!html.contains(">L<"));
        Ok(())
    }

    #[test]
    fn inactive_pending_tab_uses_the_sunk_surface_without_loading() -> TestResult {
        let tab_id = viewer_tab_id(1)?;
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ViewerTabItemProps {
            tab: ViewerTab {
                id: tab_id,
                label: "Working tree".to_owned(),
                kind: ViewerTabKind::Live,
                state: ViewerTabState::Pending,
            },
            active: false,
            rows_loading: false,
            reorderable: false,
            onactivate: EventHandler::new(|()| {}),
            onkeydown: EventHandler::new(|_| {}),
            onclose: EventHandler::new(|_| {}),
            onmove: None,
        });
        let mut tab = VirtualDom::new_with_props(ViewerTabItem, props);
        tab.rebuild_in_place();
        let html = dioxus_ssr::render(&tab);

        assert!(html.contains("bg-sunk text-ink-2"));
        assert!(html.contains("hover:bg-surface-2 hover:text-ink"));
        assert!(!html.contains("bg-transparent text-ink-2"));
        assert!(html.contains("aria-busy=\"false\""));
        assert!(html.contains(", Live"));
        assert!(!html.contains("animate-spin"));
        assert!(!html.contains("Rendering"));
        Ok(())
    }

    #[test]
    fn primary_pointer_activation_precedes_and_suppresses_its_click() {
        let mut gesture = ViewerTabActivationGesture::default();

        assert!(gesture.pointer_down(Some(MouseButton::Primary)));
        assert!(!gesture.click());
        assert!(gesture.click());
        assert!(!gesture.pointer_down(Some(MouseButton::Secondary)));
        gesture.cancel();
        assert!(gesture.click());
    }

    #[test]
    fn measurement_tab_has_no_interactive_controls() -> TestResult {
        let html = dioxus_ssr::render_element(rsx! {
            ViewerTabRailMeasurementItem {
                tab: ViewerTab {
                    id: viewer_tab_id(1)?,
                    label: "Working tree".to_owned(),
                    kind: ViewerTabKind::Live,
                    state: ViewerTabState::Ready,
                },
                rows_loading: false,
            }
        });

        assert!(html.contains("data-viewer-tab-measurement=\"true\""));
        assert!(html.contains("Working tree"));
        assert!(!html.contains("<button"));
        assert!(!html.contains("draggable"));
        Ok(())
    }

    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    #[test]
    fn overflow_panel_stays_below_its_trigger_with_bounded_height() {
        assert!(VIEWER_TAB_OVERFLOW_PANEL_CLASSES.contains("[position-area:bottom_span-left]"));
        assert!(
            VIEWER_TAB_OVERFLOW_PANEL_CLASSES.contains("max-h-[min(28rem,calc(100%-0.75rem))]")
        );
        assert!(!VIEWER_TAB_OVERFLOW_PANEL_CLASSES.contains("position-try-fallbacks"));
        assert!(VIEWER_TAB_OVERFLOW_PANEL_CLASSES.contains("open:grid"));
        assert!(
            !VIEWER_TAB_OVERFLOW_PANEL_CLASSES
                .split_whitespace()
                .any(|class| class == "grid")
        );
    }

    #[cfg(any(feature = "component-preview", feature = "desktop"))]
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
            reorderable: true,
            onactivate: EventHandler::new(|_| {}),
            onclose: EventHandler::new(|_| {}),
            onmove: EventHandler::new(|_| {}),
        });
        let mut menu = VirtualDom::new_with_props(ViewerTabOverflowMenu, props);
        menu.rebuild_in_place();
        let html = dioxus_ssr::render(&menu);

        assert!(html.contains("Choose open diff. Current: Working tree. 2 open diffs."));
        assert!(html.contains("aria-current=\"page\""));
        assert!(html.contains("Close Working tree"));
        assert!(html.contains("Close Saved comparison"));
        assert!(!html.contains("Rendering"));
        assert!(!html.contains("animate-spin"));
        assert_eq!(html.matches("draggable=\"false\"").count(), 2);
        assert_eq!(
            html.matches("aria-roledescription=\"sortable tab\"")
                .count(),
            2
        );
        assert!(!html.contains("Snapshot"));
        Ok(())
    }
}
