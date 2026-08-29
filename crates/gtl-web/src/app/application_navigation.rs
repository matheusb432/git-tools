use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{
    SetViewerPreference, ViewerActiveState, ViewerTab, ViewerTabKind, ViewerTabRequest,
    ViewerTabState, ViewerTheme,
};
use lucide_dioxus::{Ellipsis, History, LoaderCircle, Settings, TriangleAlert, X};

use super::{
    application_layout::{ViewerContext, ViewerShellLoad},
    application_router::Route,
};
use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{
            Button, ButtonSize, ButtonVariant, CountBadge, IconPopover, IconPopoverIconMotion,
            MENU_ACTION_HOST_CLASSES, MenuActionContent, ScrollArea, ScrollAreaVariant,
            ViewerThemePicker, use_toast,
        },
    },
};

const VIEWER_MENU_ID: &str = "viewer-menu";

#[component]
pub(crate) fn ApplicationNavigation() -> Element {
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let toast = use_toast();
    let shell = viewer.shell();
    let tab_ids = use_memo(move || match &*shell.read() {
        ViewerShellLoad::Ready(shell) => shell.tabs.iter().map(|tab| tab.id).collect(),
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => Vec::new(),
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
                class: "flex min-w-0 flex-1 items-end gap-1 overflow-x-auto",
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
                        let tab_label = tab.label.clone();
                        rsx! {
                            div {
                                key: "{tab.id}",
                                class: if active { "flex min-w-28 max-w-60 shrink-0 items-center rounded-t-panel border border-b-0 border-acc-line bg-bg text-ink" } else { "flex min-w-28 max-w-60 shrink-0 items-center rounded-t-panel border border-b-0 border-transparent bg-surface-2 text-ink-2 hover:border-line-2 hover:text-ink" },
                                button {
                                    id: tab_element_id(tab_id),
                                    class: "flex min-w-0 flex-1 cursor-pointer items-center gap-2 border-0 bg-transparent py-2 pr-1 pl-2.5 text-left text-inherit active:bg-acc-soft focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc",
                                    r#type: "button",
                                    role: "tab",
                                    aria_selected: active.to_string(),
                                    aria_controls: "viewer-active-view",
                                    tabindex: if active { "0" } else { "-1" },
                                    title: tab.label.clone(),
                                    onclick: move |_| {
                                        if active {
                                            navigator.push(Route::Workspace {});
                                            return;
                                        }
                                        spawn(async move {
                                            match viewer_server::activate_tab(ViewerTabRequest { tab_id }).await {
                                                Ok(shell) => {
                                                    viewer.replace_shell(shell);
                                                    navigator.push(Route::Workspace {});
                                                }
                                                Err(error) => toast.error(error.message()),
                                            }
                                        });
                                    },
                                    onkeydown: move |event| {
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
                                                spawn(async move {
                                                    match viewer_server::activate_tab(ViewerTabRequest {
                                                            tab_id: target,
                                                        })
                                                        .await
                                                    {
                                                        Ok(shell) => {
                                                            viewer.replace_shell(shell);
                                                            navigator.push(Route::Workspace {});
                                                            browser::focus_element(tab_element_id(target));
                                                        }
                                                        Err(error) => toast.error(error.message()),
                                                    }
                                                });
                                            }
                                        }
                                    },
                                    span {
                                        class: "inline-flex size-4 flex-none items-center justify-center rounded-sm border border-line-2 text-xs font-bold text-ink-3",
                                        aria_hidden: "true",
                                        {tab_kind_label(tab.kind)}
                                    }
                                    span { class: "min-w-0 truncate", "{tab.label}" }
                                    if tab.kind == ViewerTabKind::Live {
                                        span { class: "sr-only", "Live" }
                                    }
                                    TabStateMarker { state: tab.state.clone() }
                                }
                                Button {
                                    size: ButtonSize::IconCompact,
                                    variant: ButtonVariant::Ghost,
                                    class: "mr-1 text-del",
                                    aria_label: "Close {tab_label}",
                                    title: "Close tab",
                                    "data-testid": test_ids::VIEWER_TAB_CLOSE.value(),
                                    onclick: move |_| {
                                        spawn(async move {
                                            match viewer_server::close_tab(ViewerTabRequest { tab_id }).await {
                                                Ok(shell) => {
                                                    viewer.replace_shell(shell);
                                                    if let Some(focus_id) = focus_tab_id {
                                                        browser::focus_element(tab_element_id(focus_id));
                                                    } else {
                                                        browser::focus_element("workspace-heading".to_owned());
                                                    }
                                                }
                                                Err(error) => toast.error(error.message()),
                                            }
                                        });
                                    },
                                    span { aria_hidden: "true",
                                        X { size: 14 }
                                    }
                                }
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

fn tab_element_id(tab_id: ViewerTabId) -> String {
    format!("viewer-tab-{tab_id}")
}

const fn tab_kind_label(kind: ViewerTabKind) -> &'static str {
    match kind {
        ViewerTabKind::Snapshot => "S",
        ViewerTabKind::Live => "L",
    }
}

const fn tab_state_label(state: &ViewerTabState) -> &'static str {
    match state {
        ViewerTabState::Ready => "Ready",
        ViewerTabState::Pending => "Rendering",
        ViewerTabState::Broken => "Render stopped",
        ViewerTabState::Error => "Render failed",
    }
}

#[component]
fn TabStateMarker(state: ViewerTabState) -> Element {
    let label = tab_state_label(&state);

    rsx! {
        match state {
            ViewerTabState::Ready => rsx! {},
            ViewerTabState::Pending => rsx! {
                span {
                    class: "flex-none animate-spin text-acc motion-reduce:animate-none",
                    aria_hidden: "true",
                    LoaderCircle { size: 13 }
                }
            },
            ViewerTabState::Broken | ViewerTabState::Error => rsx! {
                span { class: "flex-none text-del", aria_hidden: "true",
                    TriangleAlert { size: 13 }
                }
            },
        }
        span { class: "sr-only", ", {label}" }
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

    use super::{TabMovement, close_focus_target, tab_focus_target, tab_state_label};
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

    #[test]
    fn every_tab_state_has_a_non_color_label() {
        assert_eq!(tab_state_label(&ViewerTabState::Ready), "Ready");
        assert_eq!(tab_state_label(&ViewerTabState::Pending), "Rendering");
        assert_eq!(tab_state_label(&ViewerTabState::Broken), "Render stopped");
        assert_eq!(tab_state_label(&ViewerTabState::Error), "Render failed");
    }
}
