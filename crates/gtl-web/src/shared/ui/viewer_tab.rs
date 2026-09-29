use dioxus::{html::input_data::MouseButton, prelude::*};
use gtl_models::{settings::ViewerLanguage, viewer::ViewerTabId};
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabState};
use lucide_dioxus::{Check, ListX, Pencil, Pin, Radio, RefreshCw, TriangleAlert, X};

use super::{
    Button, ButtonSize, ButtonVariant, HoverPopover, HoverPopoverPlacement, InlineTextEditor,
    InlineTextSubmission, LoadingSpinner, use_hover_popover,
};
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

#[derive(Clone, PartialEq)]
pub(crate) struct ViewerTabMenuTarget {
    pub tab_id: ViewerTabId,
    pub menu_id: String,
    pub trigger_id: String,
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
    #[props(default)] onupdate: Option<EventHandler<()>>,
    #[props(default)] onlive: Option<EventHandler<bool>>,
    #[props(default)] onpin: Option<EventHandler<bool>>,
    #[props(default)] oncloseothers: Option<EventHandler<()>>,
    #[props(default)] onrename: Option<EventHandler<InlineTextSubmission>>,
    menu_actions: Option<Callback<ViewerTabMenuTarget, Element>>,
    warning_details: Option<Element>,
) -> Element {
    let language = use_language();
    let label = recipe_label_text(&tab.label, language);
    let presentation_state = tab_presentation_state(&tab.state, rows_loading);
    let tab_id = tab.id;
    let mut editing = use_signal(|| false);
    let mut activation_gesture = use_signal(ViewerTabActivationGesture::default);
    let drag = pointer_drag::use_pointer_drag(onmove);
    let menu_id = format!("viewer-tab-{tab_id}-menu");
    let details_id = format!("viewer-tab-{tab_id}-details");
    let details_anchor = format!("--{details_id}");
    let hover = use_hover_popover(details_id.clone());
    let warning = warning_details.is_some();
    let details = rsx! {
        ViewerTabDetails { tab: tab.clone(), warning_details }
    };
    let actions = menu_actions.map(|render| {
        render.call(ViewerTabMenuTarget {
            tab_id,
            menu_id: menu_id.clone(),
            trigger_id: viewer_tab_element_id(tab_id),
        })
    });
    let mouse_menu_id = menu_id.clone();
    let keyboard_menu_id = menu_id.clone();
    let mut menu_position = use_signal(|| (16.0, 48.0));
    rsx! {
        div {
            class: "viewer-tab group/viewer-tab",
            style: "anchor-name: {details_anchor};",
            onmouseenter: move |_| {
                if !editing() {
                    hover.pointer_enter.call(());
                }
            },
            onmouseleave: move |_| hover.pointer_leave.call(()),
            onfocusin: move |_| {
                if !editing()
                    && browser::element_has_visible_focus(&viewer_tab_element_id(tab_id))
                {
                    hover.focus_enter.call(());
                }
            },
            onfocusout: move |_| hover.focus_leave.call(()),
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
                if event.key() == Key::Escape {
                    hover.pointer_leave.call(());
                    hover.focus_leave.call(());
                }
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
                div { class: "viewer-tab-trigger viewer-tab-editing",
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
                    aria_describedby: details_id.clone(),
                    tabindex: if active { "0" } else { "-1" },
                    onpointerdown: move |event: PointerEvent| {
                        if event.pointer_type() == "touch" {
                            return;
                        }
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
                    {viewer_tab_rail_content(&tab, &label, presentation_state, warning)}
                }
            }
            if tab.pinned {
                Button {
                    size: ButtonSize::IconCompact,
                    variant: ButtonVariant::Bare,
                    class: "viewer-tab-pin text-acc",
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
                    actions,
                    details: details.clone(),
                    pinned: tab.pinned,
                    live: tab.live,
                    pending: matches!(tab.state, ViewerTabState::Pending),
                    position: menu_position(),
                    onrename: onrename
                        .is_some()
                        .then(|| EventHandler::new(move |()| {
                            hover.pointer_leave.call(());
                            hover.focus_leave.call(());
                            editing.set(true);
                        })),
                    onupdate,
                    onlive,
                    onpin,
                    onclose,
                    oncloseothers,
                }
            }
            HoverPopover {
                id: details_id,
                anchor_name: details_anchor.clone(),
                aria_label: t!(language, "tab-details"),
                placement: HoverPopoverPlacement::Below,
                {details}
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

fn viewer_tab_rail_content(
    tab: &ViewerTab,
    label: &str,
    presentation_state: TabPresentationState,
    warning: bool,
) -> Element {
    rsx! {
        if presentation_state != TabPresentationState::Ready {
            TabStateMarker { state: presentation_state }
        }
        if warning {
            TabWarningMarker {}
        }
        span { class: "viewer-tab-label", "{label}" }
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
            class: "viewer-tab-close group/viewer-tab-close",
            aria_label: t!(language, "tab-close-named", tab = label),
            "data-testid": test_id,
            onclick,
            span { class: "viewer-tab-close-surface", aria_hidden: "true" }
            span { class: "viewer-tab-close-icon", aria_hidden: "true",
                X { size: 14, stroke_width: 4 }
            }
        }
    }
}

pub(crate) fn viewer_tab_element_id(tab_id: ViewerTabId) -> String {
    format!("viewer-tab-{tab_id}")
}

#[component]
fn ViewerTabDetails(tab: ViewerTab, warning_details: Option<Element>) -> Element {
    let language = use_language();
    rsx! {
        div { class: "grid min-w-0 gap-1 text-xs",
            p { class: "font-semibold text-ink break-words",
                {recipe_label_text(&tab.label, language)}
            }
            if let Some(details) = &tab.details {
                if details.comparison != tab.label {
                    p { class: "text-ink-2 break-words",
                        {recipe_label_text(&details.comparison, language)}
                    }
                }
                p { class: "text-ink-3 break-all", "{details.repository.to_string_lossy()}" }
                if let Some(range) = &details.range {
                    dl { class: "mt-1 grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-1 text-ink-3",
                        dt { {t!(language, "tab-details-base")} }
                        dd { class: "font-mono break-all", "{range.base}" }
                        dt { {t!(language, "tab-details-head")} }
                        dd { class: "font-mono break-all", "{range.head}" }
                    }
                }
            }
            if tab.live {
                p { class: "text-acc", {t!(language, "tab-live")} }
            }
            if let Some(warning_details) = warning_details {
                {warning_details}
            }
        }
    }
}

#[component]
fn TabWarningMarker() -> Element {
    rsx! {
        span { class: "inline-flex flex-none text-warn", aria_hidden: "true",
            TriangleAlert { size: 13 }
        }
        span { class: "sr-only", {t!(use_language(), "workspace-live-warnings")} }
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
        ViewerTabItem, ViewerTabItemProps, tab_presentation_state,
    };
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
            menu_actions: None,
            warning_details: None,
            onupdate: None,
            onlive: None,
            onpin: None,
            oncloseothers: None,
            tab: ViewerTab {
                details: None,
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
            menu_actions: None,
            warning_details: None,
            onupdate: None,
            onlive: None,
            onpin: None,
            oncloseothers: None,
            tab: ViewerTab {
                details: None,
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
}

#[component]
fn ViewerTabContextMenu(
    id: String,
    actions: Option<Element>,
    details: Element,
    trigger_id: String,
    pinned: bool,
    live: bool,
    pending: bool,
    position: (f64, f64),
    onupdate: Option<EventHandler<()>>,
    onlive: Option<EventHandler<bool>>,
    onpin: EventHandler<bool>,
    onclose: EventHandler<MouseEvent>,
    oncloseothers: Option<EventHandler<()>>,
    onrename: Option<EventHandler<()>>,
) -> Element {
    let language = use_language();
    let mut open = use_signal(|| false);
    let toggle_id = id.clone();
    let keyboard_id = id.clone();
    let update_id = id.clone();
    let live_id = id.clone();
    let pin_id = id.clone();
    let others_id = id.clone();
    let rename_id = id.clone();
    let (x, y) = position;
    rsx! {
        div {
            id: id.clone(),
            class: "viewer-tab-context-menu",
            style: "left:clamp(0px, {x}px, calc(100vw - 16rem));top:clamp(0px, {y}px, calc(100vh - min(30rem, 100vh)));max-height:calc(100vh - clamp(0px, {y}px, calc(100vh - min(30rem, 100vh))) - 0.5rem);",
            popover: "auto",
            role: "menu",
            aria_label: t!(language, "tab-actions"),
            ontoggle: move |_| open.set(browser::popover_is_open(&toggle_id)),
            onkeydown: move |event| super::menu_keyboard::keydown(&keyboard_id, &trigger_id, &event),
            div { class: "viewer-tab-context-details", {details} }
            if open() {
                if let Some(actions) = actions {
                    {actions}
                }
            }
            if let Some(update) = onupdate {
                button {
                    class: "control-menu-action viewer-tab-context-action",
                    r#type: "button",
                    role: "menuitem",
                    tabindex: "-1",
                    disabled: pending,
                    title: t!(language, "workspace-refresh-hint"),
                    onclick: move |_| {
                        browser::hide_popover(&update_id);
                        update.call(());
                    },
                    span { class: "inline-flex text-acc", aria_hidden: "true",
                        RefreshCw { size: 14 }
                    }
                    {t!(language, "workspace-refresh")}
                }
            }
            if let Some(set_live) = onlive {
                button {
                    class: "control-menu-action viewer-tab-context-action",
                    r#type: "button",
                    role: "menuitemcheckbox",
                    aria_checked: live.to_string(),
                    title: if live { t!(language, "workspace-live-stop-hint") } else { t!(language, "workspace-live-start-hint") },
                    tabindex: "-1",
                    onclick: move |_| {
                        browser::hide_popover(&live_id);
                        set_live.call(!live);
                    },
                    span { class: "inline-flex text-acc", aria_hidden: "true",
                        Radio { size: 14 }
                    }
                    {t!(language, "workspace-live")}
                    if live {
                        span { class: "ml-auto", aria_hidden: "true",
                            Check { size: 14 }
                        }
                    }
                }
            }
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
                    span { class: "inline-flex text-acc", aria_hidden: "true",
                        Pencil { size: 14 }
                    }
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
                span { class: "inline-flex text-acc", aria_hidden: "true",
                    Pin { size: 14 }
                }
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
                span { class: "inline-flex text-acc", aria_hidden: "true",
                    X { size: 14 }
                }
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
                span { class: "inline-flex text-acc", aria_hidden: "true",
                    ListX { size: 14 }
                }
                span { {t!(language, "tab-close-others")} }
                span { class: "ml-auto text-ink-3", aria_hidden: "true", "Alt+O" }
            }
        }
    }
}
