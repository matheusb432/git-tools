use dioxus::{html::input_data::MouseButton, prelude::*};
use gtl_models::viewer::{ViewerTabId, ViewerTabPlacement};
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabKind, ViewerTabState};
#[cfg(any(feature = "component-preview", feature = "desktop"))]
use lucide_dioxus::{Check, ChevronDown};
use lucide_dioxus::{Radio, TriangleAlert, X};

use super::{Button, ButtonSize, ButtonVariant, LoadingSpinner};
#[cfg(any(feature = "component-preview", feature = "desktop"))]
use super::{CountBadge, ScrollArea};

#[cfg(any(feature = "component-preview", feature = "desktop"))]
const VIEWER_TAB_OVERFLOW_PANEL_CLASSES: &str = "fixed inset-auto z-70 m-0 mt-1 mb-2 max-h-[min(28rem,calc(100%-0.75rem))] w-[min(26rem,calc(100vw-1rem))] origin-top-right grid-rows-[auto_minmax(0,1fr)] overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-floating [position-area:bottom_span-left] open:grid open:animate-popover-enter motion-reduce:animate-none";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ViewerTabDragPresentation {
    #[default]
    Idle,
    Dragging,
    DropTarget,
    DropBefore,
    DropAfter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewerTabDropTarget {
    pub(crate) tab_id: ViewerTabId,
    pub(crate) placement: ViewerTabPlacement,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ViewerTabDragSession {
    dragged_tab_id: Option<ViewerTabId>,
    drop_target: Option<ViewerTabDropTarget>,
}

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

impl ViewerTabDragSession {
    #[must_use]
    pub(crate) fn started(mut self, tab_id: ViewerTabId) -> Self {
        self.dragged_tab_id = Some(tab_id);
        self.drop_target = None;
        self
    }

    #[must_use]
    pub(crate) fn dragged_over(mut self, target: ViewerTabDropTarget) -> Self {
        self.drop_target = self
            .dragged_tab_id
            .filter(|dragged_tab_id| *dragged_tab_id != target.tab_id)
            .map(|_| target);
        self
    }

    #[must_use]
    pub(crate) fn cancelled(mut self) -> Self {
        self.dragged_tab_id = None;
        self.drop_target = None;
        self
    }

    pub(crate) fn presentation(self, tab_id: ViewerTabId) -> ViewerTabDragPresentation {
        if self.dragged_tab_id == Some(tab_id) {
            return ViewerTabDragPresentation::Dragging;
        }
        match self.drop_target.filter(|target| target.tab_id == tab_id) {
            Some(ViewerTabDropTarget {
                placement: ViewerTabPlacement::Before,
                ..
            }) => ViewerTabDragPresentation::DropBefore,
            Some(ViewerTabDropTarget {
                placement: ViewerTabPlacement::After,
                ..
            }) => ViewerTabDragPresentation::DropAfter,
            None if self.dragged_tab_id.is_some() => ViewerTabDragPresentation::DropTarget,
            None => ViewerTabDragPresentation::Idle,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ViewerTabDragController {
    session: Signal<ViewerTabDragSession>,
}

impl ViewerTabDragController {
    pub(crate) fn presentation(self, tab_id: ViewerTabId) -> ViewerTabDragPresentation {
        (self.session)().presentation(tab_id)
    }

    pub(crate) fn start(self, tab_id: ViewerTabId) {
        let current = *self.session.peek();
        self.publish(current.started(tab_id));
    }

    pub(crate) fn drag_over(self, target: ViewerTabDropTarget) {
        let current = *self.session.peek();
        self.publish(current.dragged_over(target));
    }

    pub(crate) fn cancel(self) {
        let current = *self.session.peek();
        self.publish(current.cancelled());
    }

    fn publish(mut self, next: ViewerTabDragSession) {
        let changed = *self.session.peek() != next;
        if changed {
            self.session.set(next);
        }
    }
}

pub(crate) fn use_viewer_tab_drag() -> ViewerTabDragController {
    ViewerTabDragController {
        session: use_signal(ViewerTabDragSession::default),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewerTabDropZoneAxis {
    Horizontal,
    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    Vertical,
}

fn viewer_tab_dragged_id(event: &DragEvent) -> Option<ViewerTabId> {
    event
        .data_transfer()
        .get_data("text/plain")?
        .parse()
        .ok()
        .and_then(|value| ViewerTabId::try_new(value).ok())
}

fn viewer_tab_move_request(
    tab_id: ViewerTabId,
    target: ViewerTabDropTarget,
) -> Option<MoveViewerTab> {
    (tab_id != target.tab_id).then_some(MoveViewerTab {
        tab_id,
        target_tab_id: target.tab_id,
        placement: target.placement,
    })
}

impl ViewerTabDropZoneAxis {
    const fn style(self, placement: ViewerTabPlacement) -> &'static str {
        match (self, placement) {
            (Self::Horizontal, ViewerTabPlacement::Before) => {
                "position:absolute;inset-block:0;left:0;width:50%;z-index:10;"
            }
            (Self::Horizontal, ViewerTabPlacement::After) => {
                "position:absolute;inset-block:0;right:0;width:50%;z-index:10;"
            }
            #[cfg(any(feature = "component-preview", feature = "desktop"))]
            (Self::Vertical, ViewerTabPlacement::Before) => {
                "position:absolute;inset-inline:0;top:0;height:50%;z-index:10;"
            }
            #[cfg(any(feature = "component-preview", feature = "desktop"))]
            (Self::Vertical, ViewerTabPlacement::After) => {
                "position:absolute;inset-inline:0;bottom:0;height:50%;z-index:10;"
            }
        }
    }
}

#[component]
fn ViewerTabDropZones(
    tab_id: ViewerTabId,
    axis: ViewerTabDropZoneAxis,
    ondragover: Option<EventHandler<ViewerTabDropTarget>>,
    ondrop: Option<EventHandler<MoveViewerTab>>,
) -> Element {
    rsx! {
        for placement in [ViewerTabPlacement::Before, ViewerTabPlacement::After] {
            span {
                key: "{placement:?}",
                style: axis.style(placement),
                "data-viewer-tab-drop-placement": match placement {
                    ViewerTabPlacement::Before => "before",
                    ViewerTabPlacement::After => "after",
                },
                aria_hidden: "true",
                ondragover: move |event: DragEvent| {
                    let Some(handler) = ondragover else {
                        return;
                    };
                    event.prevent_default();
                    event.data_transfer().set_drop_effect("move");
                    handler
                        .call(ViewerTabDropTarget {
                            tab_id,
                            placement,
                        });
                },
                ondrop: move |event: DragEvent| {
                    let Some(handler) = ondrop else {
                        return;
                    };
                    event.prevent_default();
                    if let Some(dragged_tab_id) = viewer_tab_dragged_id(&event)
                        && let Some(request) = viewer_tab_move_request(
                            dragged_tab_id,
                            ViewerTabDropTarget {
                                tab_id,
                                placement,
                            },
                        )
                    {
                        handler.call(request);
                    }
                },
            }
        }
    }
}

#[component]
pub(crate) fn ViewerTabItem(
    tab: ViewerTab,
    active: bool,
    #[props(default)] rows_loading: bool,
    #[props(default)] drag_presentation: ViewerTabDragPresentation,
    #[props(default)] reorderable: bool,
    onactivate: EventHandler<()>,
    onkeydown: EventHandler<KeyboardEvent>,
    onclose: EventHandler<MouseEvent>,
    #[props(default)] ondragstart: Option<EventHandler<ViewerTabId>>,
    #[props(default)] ondragover: Option<EventHandler<ViewerTabDropTarget>>,
    #[props(default)] ondrop: Option<EventHandler<MoveViewerTab>>,
    #[props(default)] ondragend: Option<EventHandler<()>>,
) -> Element {
    let presentation_state = tab_presentation_state(&tab.state, rows_loading);
    let tab_id = tab.id;
    let mut activation_gesture = use_signal(ViewerTabActivationGesture::default);
    let surface_classes = if active {
        "bg-surface-2 text-ink"
    } else {
        "bg-transparent text-ink-2 hover:bg-surface-2 hover:text-ink"
    };
    let drag_classes = match drag_presentation {
        ViewerTabDragPresentation::Dragging => "opacity-50",
        ViewerTabDragPresentation::Idle
        | ViewerTabDragPresentation::DropTarget
        | ViewerTabDragPresentation::DropBefore
        | ViewerTabDragPresentation::DropAfter => "opacity-100",
    };
    let activation_classes = if reorderable {
        "cursor-grab active:cursor-grabbing"
    } else {
        "cursor-pointer"
    };

    rsx! {
        div {
            class: "group/viewer-tab relative flex h-9 min-w-24 max-w-72 shrink-0 select-none items-center {surface_classes} {drag_classes}",
            "data-drag-state": match drag_presentation {
                ViewerTabDragPresentation::Idle => "idle",
                ViewerTabDragPresentation::Dragging => "dragging",
                ViewerTabDragPresentation::DropTarget => "drop-target",
                ViewerTabDragPresentation::DropBefore => "drop-before",
                ViewerTabDragPresentation::DropAfter => "drop-after",
            },
            ondragstart: move |event: DragEvent| {
                activation_gesture.write().cancel();
                let Some(handler) = ondragstart else {
                    return;
                };
                event.data_transfer().set_effect_allowed("move");
                event.data_transfer().set_drop_effect("move");
                let _ = event
                    .data_transfer()
                    .set_data("text/plain", &tab_id.to_string());
                handler.call(tab_id);
            },
            ondragend: move |_| {
                if let Some(handler) = ondragend {
                    handler.call(());
                }
            },
            button {
                id: viewer_tab_element_id(tab_id),
                class: "flex h-full min-w-0 flex-1 items-center gap-1.5 border-0 bg-transparent pr-1 pl-2 text-left text-inherit focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc {activation_classes}",
                r#type: "button",
                draggable: if reorderable { "true" } else { "false" },
                role: "tab",
                aria_roledescription: reorderable.then_some("sortable tab"),
                aria_selected: active.to_string(),
                aria_busy: presentation_state.is_loading().to_string(),
                aria_controls: "viewer-active-view",
                tabindex: if active { "0" } else { "-1" },
                title: tab.label.clone(),
                onpointerdown: move |event: PointerEvent| {
                    if activation_gesture
                        .write()
                        .pointer_down(event.trigger_button())
                    {
                        onactivate.call(());
                    }
                },
                onclick: move |_| {
                    if activation_gesture.write().click() {
                        onactivate.call(());
                    }
                },
                onkeydown: move |event| {
                    activation_gesture.write().cancel();
                    onkeydown.call(event);
                },
                onblur: move |_| activation_gesture.write().cancel(),
                onpointercancel: move |_| activation_gesture.write().cancel(),
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
            match drag_presentation {
                ViewerTabDragPresentation::DropBefore => rsx! {
                    span {
                        class: "pointer-events-none absolute inset-y-1 left-0 w-0.5 bg-acc",
                        aria_hidden: "true",
                    }
                },
                ViewerTabDragPresentation::DropAfter => rsx! {
                    span {
                        class: "pointer-events-none absolute inset-y-1 right-0 w-0.5 bg-acc",
                        aria_hidden: "true",
                    }
                },
                ViewerTabDragPresentation::Idle
                | ViewerTabDragPresentation::Dragging
                | ViewerTabDragPresentation::DropTarget => {
                    rsx! {}
                }
            }
            if matches!(
                drag_presentation,
                ViewerTabDragPresentation::DropTarget
                | ViewerTabDragPresentation::DropBefore
                | ViewerTabDragPresentation::DropAfter
            )
            {
                ViewerTabDropZones {
                    tab_id,
                    axis: ViewerTabDropZoneAxis::Horizontal,
                    ondragover,
                    ondrop,
                }
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

fn viewer_tab_rail_content(tab: &ViewerTab, presentation_state: TabPresentationState) -> Element {
    rsx! {
        ViewerTabKindIndicator { kind: tab.kind }
        span { class: "flex min-w-0 flex-1 items-center gap-1.5",
            span { class: "min-w-0 flex-1 truncate", "{tab.label}" }
            TabStateMarker { state: presentation_state }
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
    if kind == ViewerTabKind::Snapshot {
        return rsx! {};
    }

    rsx! {
        span {
            class: "inline-flex size-4 flex-none items-center justify-center text-add",
            aria_hidden: "true",
            Radio { size: 13, stroke_width: 2 }
        }
        span { class: "sr-only", ", Live" }
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
    let drag = use_viewer_tab_drag();
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
                class: "relative flex h-9 w-full min-w-0 max-w-[30rem] cursor-pointer select-none items-center gap-1.5 border-0 bg-surface-2 px-2.5 text-left text-ink hover:bg-line focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc group-has-[:popover-open]/viewer-tab-overflow:bg-line",
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
                                        drag_presentation: drag.presentation(tab_id),
                                        reorderable,
                                        onactivate,
                                        onclose,
                                        ondragstart: move |tab_id| drag.start(tab_id),
                                        ondragover: move |target| drag.drag_over(target),
                                        ondrop: move |request| {
                                            drag.cancel();
                                            onmove.call(request);
                                        },
                                        ondragend: move |()| drag.cancel(),
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
    drag_presentation: ViewerTabDragPresentation,
    reorderable: bool,
    onactivate: EventHandler<ViewerTabId>,
    onclose: EventHandler<ViewerTabId>,
    ondragstart: EventHandler<ViewerTabId>,
    ondragover: EventHandler<ViewerTabDropTarget>,
    ondrop: EventHandler<MoveViewerTab>,
    ondragend: EventHandler<()>,
) -> Element {
    let tab_id = tab.id;
    let mut activation_gesture = use_signal(ViewerTabActivationGesture::default);
    let surface_classes = if active {
        "bg-surface-2 text-ink"
    } else {
        "bg-transparent text-ink-2 hover:bg-surface-2 hover:text-ink"
    };
    let drag_classes = match drag_presentation {
        ViewerTabDragPresentation::Dragging => "opacity-50",
        ViewerTabDragPresentation::Idle
        | ViewerTabDragPresentation::DropTarget
        | ViewerTabDragPresentation::DropBefore
        | ViewerTabDragPresentation::DropAfter => "opacity-100",
    };
    let activation_classes = if reorderable {
        "cursor-grab active:cursor-grabbing"
    } else {
        "cursor-pointer"
    };

    rsx! {
        li {
            class: "group/viewer-tab-menu relative flex min-w-0 select-none items-center rounded-sm {surface_classes} {drag_classes}",
            ondragstart: move |event: DragEvent| {
                activation_gesture.write().cancel();
                event.data_transfer().set_effect_allowed("move");
                event.data_transfer().set_drop_effect("move");
                let _ = event
                    .data_transfer()
                    .set_data("text/plain", &tab_id.to_string());
                ondragstart.call(tab_id);
            },
            ondragend: move |_| ondragend.call(()),
            button {
                class: "flex min-h-10 min-w-0 flex-1 items-center gap-1.5 border-0 bg-transparent px-2 py-1 text-left text-inherit focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc {activation_classes}",
                r#type: "button",
                draggable: if reorderable { "true" } else { "false" },
                popovertarget: popover_id,
                popovertargetaction: "hide",
                aria_roledescription: reorderable.then_some("sortable tab"),
                aria_current: active.then_some("page"),
                aria_controls: "viewer-active-view",
                onpointerdown: move |event: PointerEvent| {
                    if activation_gesture
                        .write()
                        .pointer_down(event.trigger_button())
                    {
                        onactivate.call(tab_id);
                    }
                },
                onclick: move |_| {
                    if activation_gesture.write().click() {
                        onactivate.call(tab_id);
                    }
                },
                onkeydown: move |_| activation_gesture.write().cancel(),
                onblur: move |_| activation_gesture.write().cancel(),
                onpointercancel: move |_| activation_gesture.write().cancel(),
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
            match drag_presentation {
                ViewerTabDragPresentation::DropBefore => rsx! {
                    span {
                        class: "pointer-events-none absolute inset-x-1 top-0 h-0.5 bg-acc",
                        aria_hidden: "true",
                    }
                },
                ViewerTabDragPresentation::DropAfter => rsx! {
                    span {
                        class: "pointer-events-none absolute inset-x-1 bottom-0 h-0.5 bg-acc",
                        aria_hidden: "true",
                    }
                },
                ViewerTabDragPresentation::Idle
                | ViewerTabDragPresentation::Dragging
                | ViewerTabDragPresentation::DropTarget => {
                    rsx! {}
                }
            }
            if matches!(
                drag_presentation,
                ViewerTabDragPresentation::DropTarget
                | ViewerTabDragPresentation::DropBefore
                | ViewerTabDragPresentation::DropAfter
            )
            {
                ViewerTabDropZones {
                    tab_id,
                    axis: ViewerTabDropZoneAxis::Vertical,
                    ondragover: Some(ondragover),
                    ondrop: Some(ondrop),
                }
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
    use gtl_models::viewer::ViewerTabPlacement;
    use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabKind, ViewerTabState};

    use super::{
        MouseButton, TabPresentationState, TabStateMarker, ViewerTabActivationGesture,
        ViewerTabDragPresentation, ViewerTabDragSession, ViewerTabDropTarget,
        ViewerTabDropZoneAxis, ViewerTabDropZones, ViewerTabItem, ViewerTabItemProps,
        ViewerTabRailMeasurementItem, tab_presentation_state, viewer_tab_move_request,
    };
    #[cfg(any(feature = "component-preview", feature = "desktop"))]
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
            drag_presentation: ViewerTabDragPresentation::Idle,
            reorderable: false,
            onactivate: EventHandler::new(|()| {}),
            onkeydown: EventHandler::new(|_| {}),
            onclose: EventHandler::new(|_| {}),
            ondragstart: None,
            ondragover: None,
            ondrop: None,
            ondragend: None,
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
    fn drag_session_emits_one_identity_based_move() -> TestResult {
        let first = viewer_tab_id(1)?;
        let second = viewer_tab_id(2)?;
        let target = ViewerTabDropTarget {
            tab_id: second,
            placement: ViewerTabPlacement::After,
        };
        let session = ViewerTabDragSession::default().started(first);
        let targeted = session.dragged_over(target);
        assert_eq!(
            session.presentation(second),
            ViewerTabDragPresentation::DropTarget
        );
        assert_eq!(
            targeted.presentation(second),
            ViewerTabDragPresentation::DropAfter
        );
        assert_eq!(targeted.dragged_over(target), targeted);
        assert_eq!(
            viewer_tab_move_request(first, target),
            Some(MoveViewerTab {
                tab_id: first,
                target_tab_id: second,
                placement: ViewerTabPlacement::After,
            })
        );
        let cancelled = targeted.cancelled();
        assert_eq!(
            cancelled.presentation(second),
            ViewerTabDragPresentation::Idle
        );
        assert_eq!(
            cancelled.dragged_over(target).presentation(second),
            ViewerTabDragPresentation::Idle
        );
        Ok(())
    }

    #[test]
    fn drop_zones_encode_placement_without_measuring_layout() -> TestResult {
        let tab_id = viewer_tab_id(1)?;
        let html = dioxus_ssr::render_element(rsx! {
            ViewerTabDropZones {
                tab_id,
                axis: ViewerTabDropZoneAxis::Horizontal,
                ondragover: None,
                ondrop: None,
            }
        });

        assert_eq!(html.matches("data-viewer-tab-drop-placement").count(), 2);
        assert!(html.contains("data-viewer-tab-drop-placement=\"before\""));
        assert!(html.contains("data-viewer-tab-drop-placement=\"after\""));
        Ok(())
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
        assert!(html.contains("Rendering"));
        assert_eq!(html.matches("draggable=\"true\"").count(), 2);
        assert_eq!(
            html.matches("aria-roledescription=\"sortable tab\"")
                .count(),
            2
        );
        assert!(!html.contains("Snapshot"));
        Ok(())
    }
}
