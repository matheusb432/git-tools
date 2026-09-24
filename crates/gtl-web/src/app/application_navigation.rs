use dioxus::prelude::*;
use gtl_models::viewer::{
    ViewerKeybindingAction, ViewerKeybindings, ViewerKeyboardModifier, ViewerTabId,
};
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabRequest};
use wasm_bindgen::JsCast as _;

use super::{
    application_layout::{ViewerContext, ViewerShellLoad},
    application_router::Route,
    window_chrome::WindowDragExcluded,
};
use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{
            NavigationBar, ScrollArea, ScrollAreaVariant, ViewerTabItem, ViewerTabOverflowMenu,
            ViewerTabRailMeasurementItem, ViewerTabSelectionIndicator, use_toast,
            viewer_tab_element_id,
        },
    },
    views::viewer_menu::ViewerMenu,
};

const VIEWER_MENU_ID: &str = "viewer-menu";
const VIEWER_TAB_OVERFLOW_MENU_ID: &str = "viewer-tab-overflow-menu";
const VIEWER_TAB_OVERFLOW_TOLERANCE_PX: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewerTabActivation {
    tab_id: ViewerTabId,
    focus: bool,
}

#[derive(Clone, Copy)]
struct ViewerTabRailOverflow {
    overflowing: Memo<bool>,
    viewport_resized: Callback<ResizeEvent>,
    content_resized: Callback<ResizeEvent>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct ViewerTabRailMeasurements {
    content_width: Option<f64>,
    viewport_width: Option<f64>,
}

impl ViewerTabRailMeasurements {
    fn with_content_width(mut self, width: f64) -> Self {
        self.content_width = Some(width);
        self
    }

    fn with_viewport_width(mut self, width: f64) -> Self {
        self.viewport_width = Some(width);
        self
    }

    fn overflowing(self) -> bool {
        self.content_width.zip(self.viewport_width).is_some_and(
            |(content_width, viewport_width)| {
                viewer_tab_rail_overflows(content_width, viewport_width)
            },
        )
    }
}

fn publish_viewer_tab_rail_measurements(
    mut measurements: Signal<ViewerTabRailMeasurements>,
    next: ViewerTabRailMeasurements,
) {
    if *measurements.peek() == next {
        return;
    }
    measurements.set(next);
}

fn viewer_tab_resize_width(event: &ResizeEvent) -> Option<f64> {
    let width = event.data().get_content_box_size().ok()?.width;
    (width.is_finite() && width >= 0.0).then_some(width)
}

fn use_viewer_tab_rail_overflow() -> ViewerTabRailOverflow {
    let measurements = use_signal(ViewerTabRailMeasurements::default);
    let overflowing = use_memo(move || measurements.read().overflowing());
    let viewport_resized = use_callback(move |event: ResizeEvent| {
        let Some(width) = viewer_tab_resize_width(&event) else {
            return;
        };
        let next = (*measurements.peek()).with_viewport_width(width);
        publish_viewer_tab_rail_measurements(measurements, next);
    });
    let content_resized = use_callback(move |event: ResizeEvent| {
        let Some(width) = viewer_tab_resize_width(&event) else {
            return;
        };
        let next = (*measurements.peek()).with_content_width(width);
        publish_viewer_tab_rail_measurements(measurements, next);
    });

    ViewerTabRailOverflow {
        overflowing,
        viewport_resized,
        content_resized,
    }
}

fn viewer_tab_rail_overflows(content_width: f64, viewport_width: f64) -> bool {
    if !content_width.is_finite()
        || !viewport_width.is_finite()
        || content_width < 0.0
        || viewport_width < 0.0
    {
        return false;
    }
    content_width > viewport_width + VIEWER_TAB_OVERFLOW_TOLERANCE_PX
}

#[component]
pub(crate) fn ApplicationNavigation() -> Element {
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let route = use_route::<Route>();
    let toast = use_toast();
    let shell = viewer.shell();
    let diff_rows_loading_tab_id = viewer.diff_rows_loading_tab_id();
    let tab_rail_overflow = use_viewer_tab_rail_overflow();
    let mut overflow_menu_open = use_signal(|| false);
    let mut pending_tab_order = use_signal(|| None::<Vec<ViewerTabId>>);
    let mut move_tab = use_action(move |request: MoveViewerTab| async move {
        match viewer_server::move_tab(request).await {
            Ok(shell) => viewer.replace_shell(shell),
            Err(error) => toast.client_error(&error),
        }
        pending_tab_order.set(None);
        Ok::<(), std::convert::Infallible>(())
    });
    let pin_tab = use_callback(move |(tab_id, pinned)| {
        spawn(async move {
            match viewer_server::set_tab_pinned(gtl_wire::viewer::SetViewerTabPinned {
                tab_id,
                pinned,
            })
            .await
            {
                Ok(()) => match viewer_server::get_shell().await {
                    Ok(shell) => viewer.replace_shell(shell),
                    Err(error) => toast.client_error(&error),
                },
                Err(error) => toast.client_error(&error),
            }
        });
    });
    let rename_snapshot = use_callback(
        move |(tab_id, submission): (ViewerTabId, crate::shared::ui::InlineTextSubmission)| {
            spawn(async move {
                let result =
                    viewer_server::rename_snapshot(gtl_wire::viewer::RenameViewerSnapshot {
                        tab_id,
                        name: submission.value,
                    })
                    .await;
                match result {
                    Ok(()) => {
                        if let Ok(shell) = viewer_server::get_shell().await {
                            viewer.replace_shell(shell);
                        }
                        (submission.complete)(Ok(()));
                    }
                    Err(error) => {
                        toast.client_error(&error);
                        (submission.complete)(Err(error.to_string()));
                    }
                }
            });
        },
    );
    let close_others = use_callback(move |tab_id| {
        spawn(async move {
            match viewer_server::close_other_tabs(ViewerTabRequest { tab_id }).await {
                Ok(()) => match viewer_server::get_shell().await {
                    Ok(shell) => viewer.replace_shell(shell),
                    Err(error) => toast.client_error(&error),
                },
                Err(error) => toast.client_error(&error),
            }
        });
    });
    let close_tab =
        use_callback(move |(tab_id, focus): (ViewerTabId, bool)| {
            let focus_tab_id = shell.with(|shell| match shell {
                ViewerShellLoad::Ready(shell) => close_focus_target(
                    &shell.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
                    tab_id,
                ),
                _ => None,
            });
            spawn(async move {
                match viewer_server::close_tab(ViewerTabRequest { tab_id }).await {
                    Ok(shell) => {
                        viewer.replace_shell(shell);
                        if focus {
                            browser::focus_element(focus_tab_id.map_or_else(
                                || "workspace-heading".to_owned(),
                                viewer_tab_element_id,
                            ));
                        }
                    }
                    Err(error) => toast.client_error(&error),
                }
            });
        });
    let active_tab_id = route.tab_id();
    let tab_shortcut = use_callback(move |shortcut: ViewerTabShortcut| {
        let tab = shell.with(|shell| match shell {
            ViewerShellLoad::Ready(shell) => shell
                .tabs
                .iter()
                .find(|tab| Some(tab.id) == active_tab_id)
                .cloned(),
            _ => None,
        });
        if let Some(tab) = tab {
            match shortcut {
                ViewerTabShortcut::Close if !tab.pinned => close_tab.call((tab.id, true)),
                ViewerTabShortcut::Pin => pin_tab.call((tab.id, !tab.pinned)),
                ViewerTabShortcut::CloseOthers => close_others.call(tab.id),
                ViewerTabShortcut::Close => {}
            }
        }
    });
    use_viewer_tab_shortcuts(shell, tab_shortcut);
    let shell_state = shell.read();
    let tabs = match &*shell_state {
        ViewerShellLoad::Ready(shell) => shell.tabs.as_slice(),
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => &[] as &[ViewerTab],
    };
    let pending_order = pending_tab_order();
    let displayed_tabs = tabs_in_order(tabs, pending_order.as_deref());
    let displayed_tab_ids = displayed_tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
    let overflow_active_tab = active_tab_id
        .and_then(|tab_id| {
            displayed_tabs
                .iter()
                .find(|tab| tab.id == tab_id)
                .map(|tab| (*tab).clone())
        })
        .or_else(|| match &*shell_state {
            ViewerShellLoad::Ready(shell) if overflow_menu_open() => {
                let tab_id = super::application_router::active_tab_id(&shell.active)?;
                displayed_tabs
                    .iter()
                    .find(|tab| tab.id == tab_id)
                    .map(|tab| (*tab).clone())
            }
            _ => None,
        });
    let tab_rail_collapsed = overflow_menu_open()
        || ((tab_rail_overflow.overflowing)() && overflow_active_tab.is_some());
    let overflow_tabs = if tab_rail_collapsed {
        displayed_tabs
            .iter()
            .map(|tab| (*tab).clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let projects_active = matches!(route, Route::Projects {});
    let overflow_drop_tabs = displayed_tab_ids.clone();
    let pinned_tab_ids = displayed_tabs
        .iter()
        .filter(|tab| tab.pinned)
        .map(|tab| tab.id)
        .collect::<Vec<_>>();
    let overflow_pinned_tabs = pinned_tab_ids.clone();
    let reorderable = pending_order.is_none();
    let activate_viewer_tab = use_callback(move |activation: ViewerTabActivation| {
        let target = Route::Diff {
            tab_id: activation.tab_id,
        };
        if route != target {
            navigator.push(target);
        }
        if activation.focus {
            browser::focus_element(viewer_tab_element_id(activation.tab_id));
        }
    });

    rsx! {
        NavigationBar {
            bordered: false,
            aria_label: "Viewer navigation",
            leading: rsx! {
                Link {
                    to: Route::Projects {},
                    class: "viewer-tab-pinned",
                    draggable: "false",
                    aria_label: "Projects",
                    aria_current: projects_active.then_some("page"),
                    title: "Projects",
                    span {
                        class: "viewer-navigation-icon size-5 [&>svg]:size-full",
                        aria_hidden: "true",
                        dangerous_inner_html: include_str!("assets/app-icon.svg"),
                    }
                    span { class: "hidden sm:inline", "Projects" }
                    ViewerTabSelectionIndicator { active: projects_active }
                }
            },
            rail: rsx! {
                ScrollArea {
                    variant: ScrollAreaVariant::Rail,
                    class: "viewer-tab-rail",
                    role: "tablist",
                    aria_label: "Open diffs",
                    aria_hidden: tab_rail_collapsed.to_string(),
                    "data-viewer-tab-rail-mode": if tab_rail_collapsed { "measurement" } else { "interactive" },
                    onresize: move |event: ResizeEvent| {
                        tab_rail_overflow.viewport_resized.call(event);
                    },
                    div {
                        class: "flex shrink-0 items-end gap-0",
                        style: "width: max-content;",
                        "data-viewer-tab-rail-content": "true",
                        onresize: move |event: ResizeEvent| {
                            tab_rail_overflow.content_resized.call(event);
                        },
                        if tabs.is_empty() {
                            p { class: "viewer-navigation-empty h-9 px-3", "No open diffs" }
                        }
                        if tab_rail_collapsed {
                            for tab in displayed_tabs.iter().copied() {
                                ViewerTabRailMeasurementItem {
                                    key: "{tab.id}",
                                    tab: tab.clone(),
                                    rows_loading: diff_rows_loading_tab_id == Some(tab.id),
                                }
                            }
                        } else {
                            for tab in displayed_tabs.iter().copied() {
                                {
                                    let tab_id = tab.id;
                                    let active = active_tab_id == Some(tab_id);
                                    let key_tabs = displayed_tab_ids.clone();
                                    let drop_tabs = displayed_tab_ids.clone();
                                    let pinned_tabs = pinned_tab_ids.clone();
                                    rsx! {
                                        ViewerTabItem {
                                            key: "{tab.id}",
                                            tab: tab.clone(),
                                            active,
                                            onrename: move |submission| rename_snapshot.call((tab_id, submission)),
                                            onpin: move |pinned| pin_tab.call((tab_id, pinned)),
                                            oncloseothers: move |()| close_others.call(tab_id),
                                            rows_loading: diff_rows_loading_tab_id == Some(tab_id),
                                            reorderable,
                                            onactivate: move |()| {
                                                activate_viewer_tab
                                                    .call(ViewerTabActivation {
                                                        tab_id,
                                                        focus: false,
                                                    });
                                            },
                                            onkeydown: move |event: KeyboardEvent| {
                                                let movement = match event.key() {
                                                    Key::ArrowRight => Some(TabMovement::Next),
                                                    Key::ArrowLeft => Some(TabMovement::Previous),
                                                    Key::Home => Some(TabMovement::First),
                                                    Key::End => Some(TabMovement::Last),
                                                    _ => None,
                                                };
                                                if let Some(movement) = movement {
                                                    event.prevent_default();
                                                    if let Some(target) = tab_focus_target(&key_tabs, tab_id, movement) {
                                                        activate_viewer_tab
                                                            .call(ViewerTabActivation {
                                                                tab_id: target,
                                                                focus: true,
                                                            });
                                                    }
                                                }
                                            },
                                            onclose: move |_| close_tab.call((tab_id, true)),
                                            onmove: move |request: MoveViewerTab| {
                                                if pinned_tabs.contains(&request.tab_id)
                                                    == pinned_tabs.contains(&request.target_tab_id)
                                                    && let Some(order) = moved_tab_ids(&drop_tabs, request)
                                                {
                                                    pending_tab_order.set(Some(order));
                                                    move_tab.call(request);
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if tab_rail_collapsed {
                    WindowDragExcluded {
                        div { class: "viewer-navigation-overflow h-9 min-w-0",
                            ViewerTabOverflowMenu {
                                id: VIEWER_TAB_OVERFLOW_MENU_ID,
                                tabs: overflow_tabs,
                                onrename: rename_snapshot,
                                onpin: pin_tab,
                                oncloseothers: close_others,
                                active_tab: overflow_active_tab,
                                onopenchange: move |open| overflow_menu_open.set(open),
                                diff_rows_loading_tab_id,
                                reorderable,
                                onactivate: move |tab_id| {
                                    activate_viewer_tab
                                        .call(ViewerTabActivation {
                                            tab_id,
                                            focus: false,
                                        });
                                },
                                onclose: move |tab_id| close_tab.call((tab_id, false)),
                                onmove: move |request: MoveViewerTab| {
                                    if overflow_pinned_tabs.contains(&request.tab_id)
                                        != overflow_pinned_tabs.contains(&request.target_tab_id)
                                    {
                                        return;
                                    }
                                    let Some(order) = moved_tab_ids(&overflow_drop_tabs, request) else {
                                        return;
                                    };
                                    pending_tab_order.set(Some(order));
                                    move_tab.call(request);
                                },
                            }
                        }
                    }
                }
            },
            trailing: rsx! {
                WindowDragExcluded {
                    ViewerMenu {
                        id: VIEWER_MENU_ID,
                        trigger_test_id: test_ids::VIEWER_MENU_TRIGGER.value().to_owned(),
                        onsettings: move |()| {
                            navigator.push(Route::Settings {});
                        },
                    }
                }
            },
        }
    }
}

#[derive(Clone, Copy)]
enum ViewerTabShortcut {
    Close,
    Pin,
    CloseOthers,
}

impl ViewerTabShortcut {
    fn conflicts_with(self, keybindings: ViewerKeybindings) -> bool {
        let (key, modifier) = match self {
            Self::Close => ("w", ViewerKeyboardModifier::Control),
            Self::Pin => ("p", ViewerKeyboardModifier::Alt),
            Self::CloseOthers => ("o", ViewerKeyboardModifier::Alt),
        };
        let modifiers = [modifier].into_iter().collect();
        [
            ViewerKeybindingAction::SearchFiles,
            ViewerKeybindingAction::SearchTextInAllFiles,
            ViewerKeybindingAction::ToggleFilesSidebar,
            ViewerKeybindingAction::ToggleCommitsSidebar,
        ]
        .into_iter()
        .any(|action| keybindings.matches_keypress(action, key, modifiers))
    }

    fn aria(self) -> &'static str {
        match self {
            Self::Close => "Control+w",
            Self::Pin => "Alt+p",
            Self::CloseOthers => "Alt+o",
        }
    }
}

fn use_viewer_tab_shortcuts(
    shell: ReadSignal<ViewerShellLoad>,
    onshortcut: Callback<ViewerTabShortcut>,
) {
    browser::use_window_keydown(move |event| {
        if event.default_prevented()
            || event.is_composing()
            || event.shift_key()
            || event.meta_key()
        {
            return;
        }
        let shortcut = match (
            event.key().to_ascii_lowercase().as_str(),
            event.ctrl_key(),
            event.alt_key(),
        ) {
            ("w", true, false) => ViewerTabShortcut::Close,
            ("p", false, true) => ViewerTabShortcut::Pin,
            ("o", false, true) => ViewerTabShortcut::CloseOthers,
            _ => return,
        };
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let menu = document
            .query_selector(".viewer-tab-context-menu:popover-open")
            .ok()
            .flatten();
        if menu.is_none()
            && document
                .query_selector("dialog[open]")
                .ok()
                .flatten()
                .is_some()
        {
            return;
        }
        if menu.is_none()
            && shell.with(|shell| match shell {
                ViewerShellLoad::Ready(shell) => {
                    shortcut.conflicts_with(shell.preferences.keybindings)
                }
                _ => false,
            })
        {
            return;
        }
        event.prevent_default();
        event.stop_immediate_propagation();
        if event.repeat() {
            return;
        }
        let Some(menu) = menu else {
            onshortcut.call(shortcut);
            return;
        };
        if let Some(action) = menu
            .query_selector(&format!("[aria-keyshortcuts='{}']", shortcut.aria()))
            .ok()
            .flatten()
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            action.click();
        }
    });
}

fn tabs_in_order<'a>(tabs: &'a [ViewerTab], order: Option<&[ViewerTabId]>) -> Vec<&'a ViewerTab> {
    let Some(order) = order.filter(|order| order.len() == tabs.len()) else {
        return tabs.iter().collect();
    };
    let ordered = order
        .iter()
        .filter_map(|id| tabs.iter().find(|tab| tab.id == *id))
        .collect::<Vec<_>>();
    if ordered.len() == tabs.len() {
        ordered
    } else {
        tabs.iter().collect()
    }
}

fn moved_tab_ids(ids: &[ViewerTabId], request: MoveViewerTab) -> Option<Vec<ViewerTabId>> {
    let from = ids.iter().position(|id| *id == request.tab_id)?;
    let target = ids.iter().position(|id| *id == request.target_tab_id)?;
    if from == target {
        return None;
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
        return None;
    }

    let mut moved = ids.to_vec();
    let id = moved.remove(from);
    moved.insert(insertion_index, id);
    Some(moved)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabMovement {
    Next,
    Previous,
    First,
    Last,
}

fn tab_focus_target(
    ids: &[ViewerTabId],
    current: ViewerTabId,
    movement: TabMovement,
) -> Option<ViewerTabId> {
    let current_index = ids.iter().position(|id| *id == current)?;
    let target_index = match movement {
        TabMovement::Next => (current_index + 1) % ids.len(),
        TabMovement::Previous => current_index.checked_sub(1).unwrap_or(ids.len() - 1),
        TabMovement::First => 0,
        TabMovement::Last => ids.len() - 1,
    };
    ids.get(target_index).copied()
}

fn close_focus_target(ids: &[ViewerTabId], closing: ViewerTabId) -> Option<ViewerTabId> {
    let index = ids.iter().position(|id| *id == closing)?;
    ids.get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|previous| ids.get(previous)))
        .copied()
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::ViewerTabPlacement;
    use gtl_wire::viewer::MoveViewerTab;

    use super::{
        TabMovement, ViewerTabRailMeasurements, close_focus_target, moved_tab_ids,
        tab_focus_target, viewer_tab_rail_overflows,
    };
    use crate::test_support::{TestResult, viewer_tab_id};

    #[test]
    fn tab_focus_wraps_and_supports_edges() -> TestResult {
        let ids = [viewer_tab_id(4)?, viewer_tab_id(8)?, viewer_tab_id(15)?];

        assert_eq!(
            tab_focus_target(&ids, viewer_tab_id(15)?, TabMovement::Next),
            Some(viewer_tab_id(4)?)
        );
        assert_eq!(
            tab_focus_target(&ids, viewer_tab_id(4)?, TabMovement::Previous),
            Some(viewer_tab_id(15)?)
        );
        assert_eq!(
            tab_focus_target(&ids, viewer_tab_id(8)?, TabMovement::First),
            Some(viewer_tab_id(4)?)
        );
        assert_eq!(
            tab_focus_target(&ids, viewer_tab_id(8)?, TabMovement::Last),
            Some(viewer_tab_id(15)?)
        );
        Ok(())
    }

    #[test]
    fn final_tab_close_targets_the_workspace_heading() -> TestResult {
        let tabs = [viewer_tab_id(4)?];

        assert_eq!(close_focus_target(&tabs, viewer_tab_id(4)?), None);
        Ok(())
    }

    #[test]
    fn optimistic_tab_move_matches_the_requested_anchor() -> TestResult {
        let first = viewer_tab_id(1)?;
        let second = viewer_tab_id(2)?;
        let third = viewer_tab_id(3)?;
        let fourth = viewer_tab_id(4)?;

        assert_eq!(
            moved_tab_ids(
                &[first, second, third, fourth],
                MoveViewerTab {
                    tab_id: first,
                    target_tab_id: third,
                    placement: ViewerTabPlacement::After,
                },
            ),
            Some(vec![second, third, first, fourth])
        );
        Ok(())
    }

    #[test]
    fn tab_rail_collapses_only_past_its_available_width() {
        assert!(!viewer_tab_rail_overflows(640.0, 640.0));
        assert!(!viewer_tab_rail_overflows(640.5, 640.0));
        assert!(viewer_tab_rail_overflows(642.0, 640.0));
        assert!(!viewer_tab_rail_overflows(f64::NAN, 640.0));
        assert!(!viewer_tab_rail_overflows(640.0, -1.0));
        assert!(
            !ViewerTabRailMeasurements::default()
                .with_content_width(642.0)
                .overflowing()
        );
        assert!(
            ViewerTabRailMeasurements::default()
                .with_content_width(642.0)
                .with_viewport_width(640.0)
                .overflowing()
        );
    }
}
