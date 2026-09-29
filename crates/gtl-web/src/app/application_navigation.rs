use dioxus::prelude::*;
use gtl_models::viewer::{
    ViewerKeybindingAction, ViewerKeybindings, ViewerKeyboardModifier, ViewerTabId,
};
use gtl_wire::viewer::{MoveViewerTab, ViewerTab, ViewerTabRequest};
use wasm_bindgen::JsCast as _;

use super::{
    application_layout::{ViewerContext, ViewerShellLoad},
    application_router::Route,
};
use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        keyboard::native_keyboard_event_key,
        ui::{
            NavigationBar, ViewerTabItem, ViewerTabRail, ViewerTabSelectionIndicator, use_toast,
            viewer_tab_element_id,
        },
    },
};

mod tab_actions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewerTabActivation {
    tab_id: ViewerTabId,
    focus: bool,
}

#[component]
pub(crate) fn ApplicationNavigation() -> Element {
    let language = use_language();
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let route = use_route::<Route>();
    let toast = use_toast();
    let shell = viewer.shell();
    let diff_rows_loading_tab_id = viewer.diff_rows_loading_tab_id();
    let menu_actions = use_callback(|target| {
        rsx! {
            tab_actions::DiffTabActions { target }
        }
    });
    let live_errors = viewer.live_errors();
    let warnings = live_errors.with(|errors| {
        errors
            .iter()
            .filter(|(_, errors)| !errors.entries().is_empty())
            .map(|(tab_id, errors)| {
                (
                    *tab_id,
                    rsx! {
                        tab_actions::LiveWarningDetails { errors: errors.entries().to_vec() }
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>()
    });
    let mut pending_tab_order = use_signal(|| None::<Vec<ViewerTabId>>);
    let mut move_tab = use_action(move |request: MoveViewerTab| async move {
        match viewer_server::move_tab(request).await {
            Ok(shell) => viewer.replace_shell(shell),
            Err(error) => toast.client_error(&error),
        }
        pending_tab_order.set(None);
        Ok::<(), std::convert::Infallible>(())
    });
    let update_tab = use_callback(move |tab_id| viewer.update_tab(tab_id));
    let live_tab = use_callback(move |(tab_id, live)| {
        spawn(async move {
            match viewer_server::set_tab_live(gtl_wire::viewer::SetViewerTabLive { tab_id, live })
                .await
            {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => toast.client_error(&error),
            }
        });
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
                        (submission.complete)(Err(client_error_message(&error, language)));
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
    let close_tab = use_callback(move |(tab_id, focus)| viewer.close_tab(tab_id, focus));
    let active_tab_id = route.tab_id();
    let projects_active = matches!(route, Route::Projects {});
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
    let tab_shortcut = use_callback(move |shortcut: ViewerTabShortcut| {
        if let ViewerTabShortcut::Next | ViewerTabShortcut::Previous = shortcut {
            let target = shell.with(|shell| {
                let ViewerShellLoad::Ready(shell) = shell else {
                    return None;
                };
                let order = pending_tab_order.peek();
                let ids = tabs_in_order(&shell.tabs, order.as_deref())
                    .into_iter()
                    .map(|tab| tab.id)
                    .collect::<Vec<_>>();
                let movement = if matches!(shortcut, ViewerTabShortcut::Next) {
                    TabMovement::Next
                } else {
                    TabMovement::Previous
                };
                active_tab_id
                    .and_then(|current| tab_focus_target(&ids, current, movement))
                    .or_else(|| match movement {
                        TabMovement::Previous => ids.last().copied(),
                        _ => ids.first().copied(),
                    })
            });
            if let Some(tab_id) = target {
                activate_viewer_tab.call(ViewerTabActivation {
                    tab_id,
                    focus: true,
                });
            }
            return;
        }
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
                ViewerTabShortcut::Close
                | ViewerTabShortcut::Next
                | ViewerTabShortcut::Previous => {}
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
    let pinned_tab_ids = displayed_tabs
        .iter()
        .filter(|tab| tab.pinned)
        .map(|tab| tab.id)
        .collect::<Vec<_>>();
    let reorderable = pending_order.is_none();

    rsx! {
        NavigationBar {
            bordered: false,
            aria_label: t!(language, "navigation-label"),
            leading: rsx! {
                Link {
                    to: Route::Projects {},
                    class: "viewer-tab-pinned",
                    draggable: "false",
                    aria_label: t!(language, "navigation-projects"),
                    aria_current: projects_active.then_some("page"),
                    title: t!(language, "navigation-projects"),
                    ApplicationLogo {}
                    ViewerTabSelectionIndicator { active: projects_active }
                }
            },
            rail: rsx! {
                ViewerTabRail { active_tab_id,
                    div {
                        class: "flex shrink-0 items-end gap-0",
                        style: "width: max-content;",
                        "data-viewer-tab-rail-content": "true",
                        if tabs.is_empty() {
                            p { class: "viewer-navigation-empty h-9 px-3",
                                {t!(language, "navigation-no-open-diffs")}
                            }
                        }
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
                                        menu_actions,
                                        warning_details: tab.live.then(|| warnings.get(&tab_id).cloned()).flatten(),
                                        onrename: move |submission| rename_snapshot.call((tab_id, submission)),
                                        onupdate: move |()| update_tab.call(tab_id),
                                        onlive: move |live| live_tab.call((tab_id, live)),
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
            },
        }
    }
}

/// Draws the launcher icon's geometry with the active theme's surface, line, and accent.
#[component]
fn ApplicationLogo() -> Element {
    rsx! {
        span { class: "viewer-navigation-icon size-6", aria_hidden: "true",
            svg {
                class: "size-full",
                view_box: "0 0 32 32",
                width: "24",
                height: "24",
                "focusable": "false",
                rect {
                    class: "fill-surface stroke-line-2",
                    x: "1",
                    y: "1",
                    width: "30",
                    height: "30",
                    rx: "7",
                    stroke_width: ".5",
                }
                svg {
                    class: "text-acc",
                    x: "4",
                    y: "4",
                    width: "24",
                    height: "24",
                    dangerous_inner_html: include_str!("assets/repository-hub.svg"),
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ViewerTabShortcut {
    Next,
    Previous,
    Close,
    Pin,
    CloseOthers,
}

impl ViewerTabShortcut {
    fn conflicts_with(self, keybindings: ViewerKeybindings) -> bool {
        let (key, modifier) = match self {
            Self::Next | Self::Previous => return false,
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
            ViewerKeybindingAction::PushDiff,
        ]
        .into_iter()
        .any(|action| keybindings.matches_keypress(action, key, modifiers))
    }

    fn aria(self) -> &'static str {
        match self {
            Self::Next => "Control+Tab",
            Self::Previous => "Control+Shift+Tab",
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
        if event.default_prevented() || event.is_composing() || event.meta_key() {
            return;
        }
        let key = native_keyboard_event_key(&event);
        let shortcut = match (
            key.to_ascii_lowercase().as_str(),
            event.ctrl_key(),
            event.alt_key(),
            event.shift_key(),
        ) {
            ("tab", true, false, false) => ViewerTabShortcut::Next,
            ("tab", true, false, true) => ViewerTabShortcut::Previous,
            ("w", true, false, false) => ViewerTabShortcut::Close,
            ("p", false, true, false) => ViewerTabShortcut::Pin,
            ("o", false, true, false) => ViewerTabShortcut::CloseOthers,
            _ => return,
        };
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let menu = document
            .query_selector(".viewer-tab-context-menu:popover-open")
            .ok()
            .flatten();
        if document
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
        let cycling = matches!(
            shortcut,
            ViewerTabShortcut::Next | ViewerTabShortcut::Previous
        );
        if cycling && let Some(menu) = &menu {
            browser::hide_popover(&menu.id());
        }
        if cycling {
            onshortcut.call(shortcut);
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

#[cfg(test)]
mod tests {
    use gtl_models::viewer::ViewerTabPlacement;
    use gtl_wire::viewer::MoveViewerTab;

    use super::{TabMovement, moved_tab_ids, tab_focus_target};
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
}
