use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{
    SetViewerPreference, ViewerActiveState, ViewerTab, ViewerTabRequest, ViewerTheme,
};
use lucide_dioxus::{Ellipsis, History, Settings};

use super::{
    application_layout::{ViewerContext, ViewerShellLoad},
    application_router::Route,
};
use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{
            CountBadge, IconPopover, IconPopoverIconMotion, MENU_ACTION_HOST_CLASSES,
            MenuActionContent, ScrollArea, ScrollAreaVariant, ViewerTabItem, ViewerThemePicker,
            use_toast, viewer_tab_element_id,
        },
    },
};

const VIEWER_MENU_ID: &str = "viewer-menu";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewerTabActivation {
    tab_id: ViewerTabId,
    focus: bool,
}

#[component]
pub(crate) fn ApplicationNavigation() -> Element {
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let toast = use_toast();
    let shell = viewer.shell();
    let diff_rows_loading_tab_id = viewer.diff_rows_loading_tab_id();
    let tab_ids = use_memo(move || match &*shell.read() {
        ViewerShellLoad::Ready(shell) => shell.tabs.iter().map(|tab| tab.id).collect(),
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => Vec::new(),
    });
    let mut activate_tab = use_action(move |activation: ViewerTabActivation| async move {
        match viewer_server::activate_tab(ViewerTabRequest {
            tab_id: activation.tab_id,
        })
        .await
        {
            Ok(shell) => {
                viewer.replace_shell(shell);
                navigator.push(Route::Workspace {});
                if activation.focus {
                    browser::focus_element(viewer_tab_element_id(activation.tab_id));
                }
            }
            Err(error) => toast.error(error.message()),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let shell_state = shell.read();
    let (active_tab_id, tabs, theme, shell_ready) = match &*shell_state {
        ViewerShellLoad::Ready(shell) => (
            active_tab_id(&shell.active),
            shell.tabs.as_slice(),
            shell.preferences.theme,
            true,
        ),
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => {
            (None, &[] as &[ViewerTab], ViewerTheme::Dark, false)
        }
    };

    rsx! {
        nav {
            class: "z-70 flex min-w-0 shrink-0 items-end gap-2.5 border-b border-line bg-surface px-3 pt-2",
            aria_label: "Viewer navigation",
            ScrollArea {
                variant: ScrollAreaVariant::Rail,
                class: "flex min-w-0 flex-1 items-end gap-0 overflow-x-auto",
                role: "tablist",
                aria_label: "Open diffs",
                if tabs.is_empty() {
                    p { class: "mb-2 self-center px-2 text-ink-3", "No open diffs" }
                }
                for tab in &tabs {
                    {
                        let tab_id = tab.id;
                        let active = active_tab_id == Some(tab_id);
                        let focus_tab_id = close_focus_target(tabs, tab_id);
                        let key_tabs = tab_ids;
                        rsx! {
                            ViewerTabItem {
                                key: "{tab.id}",
                                tab: tab.clone(),
                                active,
                                rows_loading: diff_rows_loading_tab_id == Some(tab_id),
                                onactivate: move |_| {
                                    if active {
                                        navigator.push(Route::Workspace {});
                                        return;
                                    }
                                    activate_tab
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
                                        if let Some(target) = tab_focus_target(&key_tabs.peek(), tab_id, movement) {
                                            activate_tab
                                                .call(ViewerTabActivation {
                                                    tab_id: target,
                                                    focus: true,
                                                });
                                        }
                                    }
                                },
                                onclose: move |_| {
                                    spawn(async move {
                                        match viewer_server::close_tab(ViewerTabRequest { tab_id }).await {
                                            Ok(shell) => {
                                                viewer.replace_shell(shell);
                                                if let Some(focus_id) = focus_tab_id {
                                                    browser::focus_element(viewer_tab_element_id(focus_id));
                                                } else {
                                                    browser::focus_element("workspace-heading".to_owned());
                                                }
                                            }
                                            Err(error) => toast.error(error.message()),
                                        }
                                    });
                                },
                            }
                        }
                    }
                }
            }

            IconPopover {
                id: VIEWER_MENU_ID,
                aria_label: "Viewer menu",
                trigger_test_id: test_ids::VIEWER_MENU_TRIGGER.value(),
                icon_motion: IconPopoverIconMotion::QuarterTurn,
                icon: rsx! {
                    Ellipsis { size: 18 }
                },
                div { class: "grid gap-0.5 p-1.5",
                    Link {
                        class: MENU_ACTION_HOST_CLASSES,
                        to: Route::History {},
                        aria_label: "History",
                        "data-testid": test_ids::VIEWER_HISTORY_OPEN.value(),
                        onclick: move |_| browser::hide_popover(VIEWER_MENU_ID),
                        MenuActionContent {
                            icon: rsx! {
                                History { size: 15 }
                            },
                            label: "History",
                            description: "Browse saved renders",
                            if !tabs.is_empty() {
                                CountBadge { count: tabs.len() }
                            }
                        }
                    }
                    ViewerThemePicker {
                        theme,
                        disabled: !shell_ready || viewer.render_command_pending(),
                        onthemechange: move |theme| {
                            viewer.set_preference(SetViewerPreference::Theme(theme));
                        },
                    }
                    Link {
                        class: MENU_ACTION_HOST_CLASSES,
                        to: Route::Settings {},
                        aria_label: "User settings",
                        onclick: move |_| browser::hide_popover(VIEWER_MENU_ID),
                        MenuActionContent {
                            icon: rsx! {
                                Settings { size: 15 }
                            },
                            label: "Settings",
                            description: "Viewer defaults",
                        }
                    }
                }
            }
        }
    }
}

const fn active_tab_id(active: &ViewerActiveState) -> Option<ViewerTabId> {
    match active {
        ViewerActiveState::Empty => None,
        ViewerActiveState::Pending { tab_id }
        | ViewerActiveState::Broken { tab_id, .. }
        | ViewerActiveState::Error { tab_id, .. } => Some(*tab_id),
        ViewerActiveState::Ready { view } => Some(view.identity.tab_id),
    }
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

fn close_focus_target(tabs: &[ViewerTab], closing: ViewerTabId) -> Option<ViewerTabId> {
    let index = tabs.iter().position(|tab| tab.id == closing)?;
    tabs.get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|previous| tabs.get(previous)))
        .map(|tab| tab.id)
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::{ViewerTab, ViewerTabKind, ViewerTabState};

    use super::{TabMovement, close_focus_target, tab_focus_target};
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
        let tabs = [ViewerTab {
            id: viewer_tab_id(4)?,
            label: "Only diff".to_owned(),
            kind: ViewerTabKind::Snapshot,
            state: ViewerTabState::Ready,
        }];

        assert_eq!(close_focus_target(&tabs, viewer_tab_id(4)?), None);
        Ok(())
    }
}
