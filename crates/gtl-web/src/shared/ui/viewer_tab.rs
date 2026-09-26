use dioxus::{html::input_data::MouseButton, prelude::*};
use gtl_models::{settings::ViewerLanguage, viewer::ViewerTabId};
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabState};
#[cfg(any(feature = "component-preview", feature = "desktop"))]
use lucide_dioxus::{Check, ChevronDown};
use lucide_dioxus::{Pin, TriangleAlert, X};

use super::{
    Button, ButtonSize, ButtonVariant, InlineTextEditor, InlineTextSubmission, LoadingSpinner,
};
#[cfg(any(feature = "component-preview", feature = "desktop"))]
use super::{CountBadge, ScrollArea};
use crate::shared::{
    browser,
    i18n::{t, use_language},
    recipe_label::recipe_label_text,
};

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
    #[props(default)] onpin: Option<EventHandler<bool>>,
    #[props(default)] oncloseothers: Option<EventHandler<()>>,
    #[props(default)] onrename: Option<EventHandler<InlineTextSubmission>>,
) -> Element {
    let language = use_language();
    let label = recipe_label_text(&tab.label, language);
    let presentation_state = tab_presentation_state(&tab.state, rows_loading);
    let tab_id = tab.id;
    let mut editing = use_signal(|| false);
    let mut activation_gesture = use_signal(ViewerTabActivationGesture::default);
    let drag = pointer_drag::use_pointer_drag(onmove);
    let menu_id = format!("viewer-tab-{tab_id}-menu");
    let mouse_menu_id = menu_id.clone();
    let keyboard_menu_id = menu_id.clone();
    let mut menu_position = use_signal(|| (16.0, 48.0));
    rsx! {
        div {
            class: "viewer-tab group/viewer-tab",
            "data-active": active.to_string(),
            "data-viewer-tab-id": "{tab_id}",
            "data-viewer-tab-axis": "horizontal",
            oncontextmenu: move |event| {
                if onpin.is_some() {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            onmouseup: move |event| {
                if onpin.is_some() && event.trigger_button() == Some(MouseButton::Secondary) {
                    event.stop_propagation();
                    let point = event.client_coordinates();
                    menu_position.set((point.x, point.y));
                    browser::show_popover(&mouse_menu_id);
                }
            },
            onkeydown: move |event: KeyboardEvent| {
                if onpin.is_some()
                    && (event.key() == Key::ContextMenu
                        || (event.key() == Key::F10
                            && event.modifiers().contains(Modifiers::SHIFT)))
                {
                    event.prevent_default();
                    browser::show_popover(&keyboard_menu_id);
                }
            },
            if editing() {
                div { class: "viewer-tab-trigger",
                    InlineTextEditor {
                        initial_value: tab.custom_name.clone().unwrap_or_default(),
                        label: t!(language, "tab-snapshot-name"),
                        placeholder: t!(language, "tab-snapshot-name"),
                        width_text: label.clone(),
                        onsubmit: move |submission| {
                            if let Some(rename) = onrename {
                                rename.call(submission);
                            }
                        },
                        onfinish: move |focus| {
                            editing.set(false);
                            if focus {
                                browser::focus_element(viewer_tab_element_id(tab_id));
                            }
                        },
                    }
                }
            } else {
                button {
                    id: viewer_tab_element_id(tab_id),
                    class: "viewer-tab-trigger",
                    "data-reorderable": reorderable.to_string(),
                    r#type: "button",
                    draggable: "false",
                    role: "tab",
                    aria_roledescription: reorderable.then(|| t!(language, "tab-sortable")),
                    aria_selected: active.to_string(),
                    aria_busy: presentation_state.is_loading().to_string(),
                    "data-viewer-state": presentation_state.dom_state(),
                    aria_controls: "viewer-active-view",
                    tabindex: if active { "0" } else { "-1" },
                    title: tab_description(&label, tab.live, language),
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
                    {viewer_tab_rail_content(&tab, &label, presentation_state)}
                }
            }
            if tab.pinned {
                Button {
                    size: ButtonSize::IconCompact,
                    variant: ButtonVariant::Bare,
                    class: "mr-0.5 text-acc",
                    aria_label: t!(language, "tab-unpin-named", tab = label.as_str()),
                    title: t!(language, "tab-unpin"),
                    onclick: move |_| {
                        if let Some(onpin) = onpin {
                            onpin.call(false);
                        }
                    },
                    Pin { size: 14 }
                }
            } else {
                ViewerTabCloseButton {
                    label: label.clone(),
                    test_id: gtl_web_contracts::test_ids::VIEWER_TAB_CLOSE.value().to_owned(),
                    onclick: onclose,
                }
            }
            if let Some(onpin) = onpin {
                ViewerTabContextMenu {
                    id: menu_id,
                    trigger_id: viewer_tab_element_id(tab_id),
                    pinned: tab.pinned,
                    position: menu_position(),
                    onrename: onrename.is_some().then(|| EventHandler::new(move |()| editing.set(true))),
                    onpin,
                    onclose,
                    oncloseothers,
                }
            }
            ViewerTabSelectionIndicator { active }
        }
    }
}

#[component]
pub(crate) fn ViewerTabSelectionIndicator(active: bool) -> Element {
    rsx! {
        span {
            class: "viewer-tab-selection-indicator",
            "data-active": active.to_string(),
            style: active.then_some("transition-duration:75ms;"),
            "data-viewer-tab-selection-indicator": "true",
            aria_hidden: "true",
        }
    }
}

#[component]
pub(crate) fn ViewerTabRailMeasurementItem(
    tab: ViewerTab,
    #[props(default)] rows_loading: bool,
) -> Element {
    let label = recipe_label_text(&tab.label, use_language());
    let presentation_state = tab_presentation_state(&tab.state, rows_loading);

    rsx! {
        div {
            class: "flex h-9 min-w-24 max-w-80 shrink-0 select-none items-center",
            "data-viewer-tab-measurement": "true",
            aria_hidden: "true",
            span { class: "flex h-full min-w-0 flex-1 items-center gap-1 pr-0.5 pl-1.5",
                {viewer_tab_rail_content(&tab, &label, presentation_state)}
            }
            span { class: "mr-0.5 size-6 flex-none", aria_hidden: "true" }
        }
    }
}

/// Describes a tab in its tooltip; a live tab says so there rather than in its label.
fn tab_description(label: &str, live: bool, language: ViewerLanguage) -> String {
    if live {
        format!("{label} - {}", t!(language, "tab-live"))
    } else {
        label.to_owned()
    }
}

fn viewer_tab_rail_content(
    tab: &ViewerTab,
    label: &str,
    presentation_state: TabPresentationState,
) -> Element {
    rsx! {
        if presentation_state != TabPresentationState::Ready {
            TabStateMarker { state: presentation_state }
        }
        span { class: "min-w-0 truncate", "{label}" }
        if tab.live {
            span { class: "sr-only", {format!(", {}", t!(use_language(), "tab-live"))} }
        }
    }
}

#[component]
fn ViewerTabCloseButton(
    label: String,
    #[props(default)] test_id: Option<String>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let language = use_language();
    rsx! {
        Button {
            size: ButtonSize::IconCompact,
            variant: ButtonVariant::Bare,
            class: "viewer-tab-close mr-0.5 group/viewer-tab-close",
            aria_label: t!(language, "tab-close-named", tab = label),
            "data-testid": test_id,
            onclick,
            span {
                class: "viewer-tab-close-surface m-auto size-5",
                style: "background-color:#c1121f;transition-duration:100ms;",
                aria_hidden: "true",
            }
            span {
                class: "viewer-tab-close-icon",
                style: "transform:translateX(-0.5px);",
                aria_hidden: "true",
                X { size: 14, stroke_width: 4 }
            }
        }
    }
}

#[cfg(any(feature = "component-preview", feature = "desktop"))]
#[component]
pub(crate) fn ViewerTabOverflowMenu(
    id: String,
    tabs: Vec<ViewerTab>,
    active_tab: Option<ViewerTab>,
    #[props(default)] diff_rows_loading_tab_id: Option<ViewerTabId>,
    #[props(default = true)] reorderable: bool,
    onactivate: EventHandler<ViewerTabId>,
    onclose: EventHandler<ViewerTabId>,
    onmove: EventHandler<MoveViewerTab>,
    #[props(default)] onpin: Option<EventHandler<(ViewerTabId, bool)>>,
    #[props(default)] oncloseothers: Option<EventHandler<ViewerTabId>>,
    #[props(default)] onrename: Option<EventHandler<(ViewerTabId, InlineTextSubmission)>>,
    #[props(default)] onopenchange: Option<EventHandler<bool>>,
) -> Element {
    let language = use_language();
    let active_tab_state = active_tab
        .as_ref()
        .map(|tab| tab_presentation_state(&tab.state, diff_rows_loading_tab_id == Some(tab.id)));
    let active_tab_id = active_tab.as_ref().map(|tab| tab.id);
    let active_tab_label = active_tab.as_ref().map_or_else(
        || t!(language, "navigation-open-diffs"),
        |tab| recipe_label_text(&tab.label, language),
    );
    let popover_id = id.clone();
    let trigger_id = format!("{id}-trigger");
    let title_id = format!("{id}-title");
    let trigger_label = t!(
        language,
        "tabs-overflow-trigger",
        current = active_tab_label.as_str(),
        count = tabs.len()
    );

    rsx! {
        div { class: "viewer-tab-overflow min-w-0 group/viewer-tab-overflow",
            button {
                id: trigger_id,
                class: "viewer-tab-overflow-trigger h-9 w-full min-w-0 gap-1.5 px-2.5",
                r#type: "button",
                popovertarget: id.clone(),
                popovertargetaction: "toggle",
                aria_label: trigger_label.clone(),
                aria_busy: active_tab_state.is_some_and(TabPresentationState::is_loading).to_string(),
                aria_controls: id.clone(),
                title: trigger_label,
                "data-testid": gtl_web_contracts::test_ids::VIEWER_TAB_OVERFLOW_TRIGGER.value(),
                span { class: "min-w-0 flex-1 truncate font-medium", "{active_tab_label}" }
                if let Some(state) = active_tab_state {
                    TabStateMarker { state }
                }
                CountBadge { count: tabs.len(), aria_hidden: "true" }
                span {
                    class: "viewer-tab-overflow-chevron",
                    style: "transition-duration:100ms;",
                    aria_hidden: "true",
                    ChevronDown { size: 15 }
                }
                ViewerTabSelectionIndicator { active: true }
            }
            div {
                id,
                class: "viewer-tab-overflow-panel m-0 mt-1 mb-2 p-0",
                style: "height: fit-content;",
                popover: "auto",
                ontoggle: move |_| {
                    if let Some(onopenchange) = onopenchange {
                        onopenchange.call(browser::popover_is_open(&popover_id));
                    }
                },
                role: "group",
                aria_labelledby: title_id.clone(),
                "data-testid": gtl_web_contracts::test_ids::VIEWER_TAB_OVERFLOW_MENU.value(),
                header { class: "viewer-tab-overflow-header gap-3 px-3 py-2.5",
                    div { class: "min-w-0 flex-1",
                        h2 {
                            id: title_id,
                            class: "text-sm font-semibold text-ink",
                            {t!(language, "navigation-open-diffs")}
                        }
                        p { class: "text-xs text-ink-3", {t!(language, "tabs-overflow-hint")} }
                    }
                    CountBadge {
                        count: tabs.len(),
                        aria_label: t!(language, "tabs-open-count", count = tabs.len()),
                    }
                }
                ScrollArea { class: "min-h-0 overscroll-contain overflow-y-auto p-1.5",
                    if tabs.is_empty() {
                        p { class: "px-3 py-2 text-xs text-ink-3",
                            {t!(language, "navigation-no-open-diffs")}
                        }
                    }
                    ul { class: "grid gap-px", role: "list",
                        for tab in &tabs {
                            {
                                let tab_id = tab.id;
                                let active = active_tab_id == Some(tab_id);
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
                                        onpin,
                                        oncloseothers,
                                        onrename,
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
    #[props(default)] onpin: Option<EventHandler<(ViewerTabId, bool)>>,
    #[props(default)] oncloseothers: Option<EventHandler<ViewerTabId>>,
    #[props(default)] onrename: Option<EventHandler<(ViewerTabId, InlineTextSubmission)>>,
) -> Element {
    let language = use_language();
    let label = recipe_label_text(&tab.label, language);
    let tab_id = tab.id;
    let mut editing = use_signal(|| false);
    let mut activation_gesture = use_signal(ViewerTabActivationGesture::default);
    let drag = pointer_drag::use_pointer_drag(Some(onmove));
    let menu_id = format!("viewer-overflow-tab-{tab_id}-menu");
    let mouse_menu_id = menu_id.clone();
    let keyboard_menu_id = menu_id.clone();
    let mut menu_position = use_signal(|| (16.0, 48.0));
    rsx! {
        li {
            class: "viewer-tab-menu-item group/viewer-tab-menu",
            "data-active": active.to_string(),
            "data-viewer-tab-id": "{tab_id}",
            "data-viewer-tab-axis": "vertical",
            oncontextmenu: move |event| {
                if onpin.is_some() {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            onmouseup: move |event| {
                if onpin.is_some() && event.trigger_button() == Some(MouseButton::Secondary) {
                    event.stop_propagation();
                    let point = event.client_coordinates();
                    menu_position.set((point.x, point.y));
                    browser::show_popover(&mouse_menu_id);
                }
            },
            onkeydown: move |event: KeyboardEvent| {
                if onpin.is_some()
                    && (event.key() == Key::ContextMenu
                        || (event.key() == Key::F10
                            && event.modifiers().contains(Modifiers::SHIFT)))
                {
                    event.prevent_default();
                    browser::show_popover(&keyboard_menu_id);
                }
            },
            if editing() {
                div { class: "viewer-tab-menu-trigger",
                    InlineTextEditor {
                        initial_value: tab.custom_name.clone().unwrap_or_default(),
                        label: t!(language, "tab-snapshot-name"),
                        placeholder: t!(language, "tab-snapshot-name"),
                        width_text: label.clone(),
                        onsubmit: move |submission| {
                            if let Some(rename) = onrename {
                                rename.call((tab_id, submission));
                            }
                        },
                        onfinish: move |focus| {
                            editing.set(false);
                            if focus {
                                browser::focus_element(format!("viewer-overflow-tab-{tab_id}-trigger"));
                            }
                        },
                    }
                }
            } else {
                button {
                    id: format!("viewer-overflow-tab-{tab_id}-trigger"),
                    class: "viewer-tab-menu-trigger",
                    "data-reorderable": reorderable.to_string(),
                    r#type: "button",
                    draggable: "false",
                    popovertarget: popover_id,
                    popovertargetaction: "hide",
                    aria_roledescription: reorderable.then(|| t!(language, "tab-sortable")),
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
                    strong { class: "min-w-0 flex-1 truncate text-xs font-semibold text-inherit",
                        "{label}"
                    }
                    span { class: "flex flex-none items-center gap-1.5",
                        TabStateMarker { state: presentation_state }
                        if let Some(label) = presentation_state.menu_label(language) {
                            small {
                                class: "text-xs text-ink-3",
                                aria_hidden: "true",
                                "{label}"
                            }
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
            }
            if tab.pinned {
                Button {
                    size: ButtonSize::IconCompact,
                    variant: ButtonVariant::Bare,
                    aria_label: t!(language, "tab-unpin-named", tab = label.as_str()),
                    onclick: move |_| {
                        if let Some(onpin) = onpin {
                            onpin.call((tab_id, false));
                        }
                    },
                    Pin { size: 14 }
                }
            } else {
                ViewerTabCloseButton {
                    label: label.clone(),
                    onclick: move |_| onclose.call(tab_id),
                }
            }
            if let Some(onpin) = onpin {
                ViewerTabContextMenu {
                    id: menu_id,
                    trigger_id: format!("viewer-overflow-tab-{tab_id}-trigger"),
                    pinned: tab.pinned,
                    position: menu_position(),
                    onrename: onrename.is_some().then(|| EventHandler::new(move |()| editing.set(true))),
                    onpin: move |pinned| onpin.call((tab_id, pinned)),
                    onclose: move |_| onclose.call(tab_id),
                    oncloseothers: move |()| {
                        if let Some(close) = oncloseothers {
                            close.call(tab_id);
                        }
                    },
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
    fn label(self, language: ViewerLanguage) -> String {
        match self {
            Self::Ready => t!(language, "tab-state-ready"),
            Self::Loading => t!(language, "tab-state-rendering"),
            Self::Broken => t!(language, "tab-state-stopped"),
            Self::Error => t!(language, "tab-state-failed"),
        }
    }

    const fn is_loading(self) -> bool {
        matches!(self, Self::Loading)
    }

    const fn dom_state(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Loading => "loading",
            Self::Broken | Self::Error => "error",
        }
    }

    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    fn menu_label(self, language: ViewerLanguage) -> Option<String> {
        match self {
            Self::Ready => None,
            Self::Loading | Self::Broken | Self::Error => Some(self.label(language)),
        }
    }
}

const fn tab_presentation_state(
    state: &ViewerTabState,
    diff_rows_loading: bool,
) -> TabPresentationState {
    match state {
        ViewerTabState::Pending => TabPresentationState::Loading,
        ViewerTabState::Ready if diff_rows_loading => TabPresentationState::Loading,
        ViewerTabState::Ready => TabPresentationState::Ready,
        ViewerTabState::Broken => TabPresentationState::Broken,
        ViewerTabState::Error => TabPresentationState::Error,
    }
}

#[component]
fn TabStateMarker(state: TabPresentationState) -> Element {
    let label = state.label(use_language());

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
    use gtl_wire::viewer::{ViewerTab, ViewerTabState};

    use super::{
        MouseButton, TabPresentationState, TabStateMarker, ViewerTabActivationGesture,
        ViewerTabItem, ViewerTabItemProps, ViewerTabRailMeasurementItem, tab_presentation_state,
    };
    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    use super::{ViewerTabOverflowMenu, ViewerTabOverflowMenuProps};
    use crate::test_support::{TestResult, recipe_label, viewer_tab_id};

    #[test]
    fn typed_and_row_stream_states_map_to_dom_states() {
        assert_eq!(
            tab_presentation_state(&ViewerTabState::Pending, false),
            TabPresentationState::Loading
        );
        assert_eq!(
            tab_presentation_state(&ViewerTabState::Pending, true),
            TabPresentationState::Loading
        );
        assert_eq!(
            tab_presentation_state(&ViewerTabState::Ready, false),
            TabPresentationState::Ready
        );
        assert_eq!(
            tab_presentation_state(&ViewerTabState::Ready, true),
            TabPresentationState::Loading
        );
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
        assert_eq!(TabPresentationState::Ready.dom_state(), "ready");
        assert_eq!(TabPresentationState::Loading.dom_state(), "loading");
        assert_eq!(TabPresentationState::Broken.dom_state(), "error");
        assert_eq!(TabPresentationState::Error.dom_state(), "error");
    }

    #[test]
    fn every_tab_state_has_a_non_color_label() {
        let language = gtl_models::settings::ViewerLanguage::EnUs;
        assert_eq!(TabPresentationState::Ready.label(language), "Ready");
        assert_eq!(TabPresentationState::Loading.label(language), "Rendering");
        assert_eq!(
            TabPresentationState::Broken.label(language),
            "Render stopped"
        );
        assert_eq!(TabPresentationState::Error.label(language), "Render failed");
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
        let label = recipe_label("Working tree")?;
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ViewerTabItemProps {
            onrename: None,
            onpin: None,
            oncloseothers: None,
            tab: ViewerTab {
                custom_name: None,
                pinned: false,
                id: tab_id,
                label,
                live: true,
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

        assert!(html.starts_with("<div class=\"viewer-tab group/viewer-tab\""));
        assert!(html.contains("data-active=\"true\""));
        assert!(html.contains("class=\"viewer-tab-selection-indicator\" data-active=\"true\""));
        assert!(html.contains("style=\"transition-duration:75ms;\""));
        assert!(!html.contains("duration-150"));
        assert!(!html.contains("active:bg-"));
        assert!(html.contains("viewer-tab-close mr-0.5 group/viewer-tab-close"));
        assert!(html.contains("background-color:#c1121f;transition-duration:100ms"));
        assert!(html.contains("viewer-tab-close-icon"));
        assert!(html.contains("transform:translateX(-0.5px)"));
        assert!(html.contains("stroke-width=\"4\""));
        assert!(html.contains("aria-label=\"Close Working tree\""));
        assert!(html.contains("data-viewer-state=\"ready\""));
        assert!(!html.contains("title=\"Close tab\""));
        assert!(html.contains(", Live"));
        assert!(!html.contains(">L<"));
        Ok(())
    }

    #[test]
    fn inactive_pending_tab_exposes_its_loading_state() -> TestResult {
        let tab_id = viewer_tab_id(1)?;
        let label = recipe_label("Working tree")?;
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ViewerTabItemProps {
            onrename: None,
            onpin: None,
            oncloseothers: None,
            tab: ViewerTab {
                custom_name: None,
                pinned: false,
                id: tab_id,
                label,
                live: true,
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

        assert!(html.contains("data-active=\"false\""));
        assert!(html.contains("class=\"viewer-tab group/viewer-tab\""));
        assert!(html.contains("aria-busy=\"true\""));
        assert!(html.contains("data-viewer-state=\"loading\""));
        assert!(html.contains("control-loading-spinner"));
        assert!(html.contains("Rendering"));
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
                    custom_name: None,
                    pinned: false,
                    id: viewer_tab_id(1)?,
                    label: recipe_label("Working tree")?,
                    live: true,
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

    #[test]
    fn a_ready_tab_starts_its_label_without_an_icon_or_state_slot() -> TestResult {
        let tab = |state| -> TestResult<ViewerTab> {
            Ok(ViewerTab {
                custom_name: None,
                pinned: false,
                id: viewer_tab_id(1)?,
                label: recipe_label("Working tree")?,
                live: true,
                state,
            })
        };
        let render = |tab: ViewerTab| {
            dioxus_ssr::render_element(rsx! {
                ViewerTabRailMeasurementItem { tab, rows_loading: false }
            })
        };

        let ready = render(tab(ViewerTabState::Ready)?);
        let pending = render(tab(ViewerTabState::Pending)?);

        assert!(!ready.contains("<svg"));
        assert!(!ready.contains("size-3.5 flex-none"));
        assert!(pending.contains("size-3.5 flex-none"));
        Ok(())
    }

    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    #[test]
    fn overflow_panel_stays_below_its_trigger_with_bounded_height() {
        let stylesheet = include_str!("../../app/assets/styles/viewer-tabs.css");
        let (_, styles) = stylesheet
            .split_once(".viewer-tab-overflow-panel {")
            .unwrap();
        let styles = styles.split('}').next().unwrap();
        assert!(styles.contains("[position-area:bottom_span-left]"));
        assert!(styles.contains("max-h-[min(28rem,calc(100%-0.75rem))]"));
        assert!(!styles.contains("position-try-fallbacks"));
        assert!(styles.contains("open:grid"));
        assert!(!styles.split_whitespace().any(|class| class == "grid"));
    }

    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    #[test]
    fn overflow_menu_exposes_current_pending_and_close_states() -> TestResult {
        let active_tab = ViewerTab {
            custom_name: None,
            pinned: false,
            id: viewer_tab_id(1)?,
            label: recipe_label("Working tree")?,
            live: true,
            state: ViewerTabState::Ready,
        };
        let pending_tab = ViewerTab {
            custom_name: None,
            pinned: false,
            id: viewer_tab_id(2)?,
            label: recipe_label("Saved comparison")?,
            live: false,
            state: ViewerTabState::Pending,
        };
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ViewerTabOverflowMenuProps {
            onopenchange: None,
            onrename: None,
            onpin: None,
            oncloseothers: None,
            id: "viewer-tab-overflow-test".to_owned(),
            tabs: vec![active_tab.clone(), pending_tab],
            active_tab: Some(active_tab),
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
        assert!(html.contains("control-loading-spinner"));
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

#[component]
fn ViewerTabContextMenu(
    id: String,
    trigger_id: String,
    pinned: bool,
    position: (f64, f64),
    onpin: EventHandler<bool>,
    onclose: EventHandler<MouseEvent>,
    oncloseothers: Option<EventHandler<()>>,
    onrename: Option<EventHandler<()>>,
) -> Element {
    let language = use_language();
    let keyboard_id = id.clone();
    let pin_id = id.clone();
    let others_id = id.clone();
    let rename_id = id.clone();
    let (x, y) = position;
    rsx! {
        div {
            id: id.clone(),
            class: "viewer-tab-context-menu",
            style: "left:clamp(0px, {x}px, calc(100vw - 13rem));top:clamp(0px, {y}px, calc(100vh - 8rem));",
            popover: "auto",
            role: "menu",
            aria_label: t!(language, "tab-actions"),
            onkeydown: move |event| super::menu_keyboard::keydown(&keyboard_id, &trigger_id, &event),
            if let Some(rename) = onrename {
                button {
                    class: "control-menu-action viewer-tab-context-action",
                    r#type: "button",
                    role: "menuitem",
                    tabindex: "-1",
                    onclick: move |_| {
                        browser::hide_popover(&rename_id);
                        rename.call(());
                    },
                    {t!(language, "tab-rename-snapshot")}
                }
            }
            button {
                class: "control-menu-action viewer-tab-context-action",
                r#type: "button",
                role: "menuitem",
                aria_keyshortcuts: "Alt+p",
                autofocus: true,
                tabindex: "-1",
                onclick: move |_| {
                    browser::hide_popover(&pin_id);
                    onpin.call(!pinned);
                },
                span {
                    if pinned {
                        {t!(language, "tab-unpin")}
                    } else {
                        {t!(language, "tab-pin")}
                    }
                }
                span { class: "ml-auto text-ink-3", aria_hidden: "true", "Alt+P" }
            }
            button {
                class: "control-menu-action viewer-tab-context-action",
                r#type: "button",
                role: "menuitem",
                aria_keyshortcuts: "Control+w",
                disabled: pinned,
                tabindex: "-1",
                onclick: move |event| {
                    browser::hide_popover(&id);
                    onclose.call(event);
                },
                span { {t!(language, "tab-close")} }
                span { class: "ml-auto text-ink-3", aria_hidden: "true", "Ctrl+W" }
            }
            button {
                class: "control-menu-action viewer-tab-context-action",
                r#type: "button",
                role: "menuitem",
                aria_keyshortcuts: "Alt+o",
                disabled: oncloseothers.is_none(),
                tabindex: "-1",
                onclick: move |_| {
                    browser::hide_popover(&others_id);
                    if let Some(close) = oncloseothers {
                        close.call(());
                    }
                },
                span { {t!(language, "tab-close-others")} }
                span { class: "ml-auto text-ink-3", aria_hidden: "true", "Alt+O" }
            }
        }
    }
}
