use dioxus::prelude::*;
use gtl_contracts::viewer::{
    SetViewerPreference, ViewerActiveState, ViewerTab, ViewerTabKind, ViewerTabState, ViewerTheme,
};
use lucide_dioxus::{CircleDot, History, LoaderCircle, Settings, TriangleAlert, X};

use super::{
    application_layout::{ViewerContext, ViewerShellLoad},
    application_router::Route,
};
use crate::{
    entities::diffs::{ViewerApi, theme_from_value, theme_label, theme_value},
    shared::{
        bridge::ClientApiError,
        browser,
        ui::{
            Button, ButtonSize, ButtonVariant, FloatingNotice, FloatingNoticeState, ScrollArea,
            ScrollAreaVariant,
        },
    },
};

const NAVIGATION_ACTION_CLASSES: &str = "mb-2 inline-flex h-8 flex-none items-center gap-2 rounded-sm border border-transparent bg-transparent px-2.5 text-ink-2 hover:border-line-2 hover:bg-surface-2 hover:text-ink active:border-acc-line active:bg-acc-soft focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";
const NAVIGATION_ACTION_ACTIVE_CLASSES: &str = "mb-2 inline-flex h-8 flex-none items-center gap-2 rounded-sm border border-acc-line bg-acc-soft px-2.5 text-acc active:border-acc active:bg-acc-soft focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";

#[component]
pub(crate) fn ApplicationNavigation() -> Element {
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let route = use_route::<Route>();
    let mut action_error = use_signal(|| None::<ClientApiError>);
    let shell = match viewer.read() {
        ViewerShellLoad::Ready(shell) => Some(shell),
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => None,
    };
    let active_tab_id = shell
        .as_ref()
        .and_then(|shell| active_tab_id(&shell.active));
    let tabs = shell
        .as_ref()
        .map_or_else(Vec::new, |shell| shell.tabs.clone());
    let theme = shell
        .as_ref()
        .map_or(ViewerTheme::Dark, |shell| shell.preferences.theme);

    rsx! {
        nav {
            class: "z-70 flex min-w-0 shrink-0 items-end gap-2.5 border-b border-line bg-surface px-3 pt-2",
            aria_label: "Open diffs",
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
                        let focus_tab_id = close_focus_target(&tabs, tab_id);
                        let key_tabs = tabs.clone();
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
                                        action_error.set(None);
                                        if active {
                                            navigator.push(Route::Workspace {});
                                            return;
                                        }
                                        spawn(async move {
                                            match ViewerApi::activate_tab(tab_id).await {
                                                Ok(shell) => {
                                                    viewer.replace_shell(shell);
                                                    navigator.push(Route::Workspace {});
                                                }
                                                Err(error) => action_error.set(Some(error)),
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
                                            let ids = key_tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
                                            if let Some(target) = tab_focus_target(&ids, tab_id, movement) {
                                                spawn(async move {
                                                    match ViewerApi::activate_tab(target).await {
                                                        Ok(shell) => {
                                                            viewer.replace_shell(shell);
                                                            navigator.push(Route::Workspace {});
                                                            browser::focus_element(tab_element_id(target));
                                                        }
                                                        Err(error) => action_error.set(Some(error)),
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
                                    onclick: move |_| {
                                        action_error.set(None);
                                        spawn(async move {
                                            match ViewerApi::close_tab(tab_id).await {
                                                Ok(shell) => {
                                                    viewer.replace_shell(shell);
                                                    if let Some(focus_id) = focus_tab_id {
                                                        browser::focus_element(tab_element_id(focus_id));
                                                    } else {
                                                        browser::focus_element("workspace-heading".to_owned());
                                                    }
                                                }
                                                Err(error) => action_error.set(Some(error)),
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

            Link {
                class: if matches!(route, Route::History {}) { NAVIGATION_ACTION_ACTIVE_CLASSES } else { NAVIGATION_ACTION_CLASSES },
                to: Route::History {},
                aria_label: "History",
                aria_current: matches!(route, Route::History {}).then_some("page"),
                span { aria_hidden: "true",
                    History { size: 14 }
                }
                "History"
                if !tabs.is_empty() {
                    span { class: "min-w-5 rounded-full bg-acc-soft px-1 text-center text-xs text-acc",
                        "{tabs.len()}"
                    }
                }
            }

            label { class: "mb-2 flex h-8 flex-none items-center gap-2 rounded-sm border border-transparent bg-transparent px-2.5 text-ink-2 hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-within:border-acc-line has-[select:disabled]:cursor-not-allowed has-[select:disabled]:opacity-50",
                span { class: "text-acc", aria_hidden: "true",
                    CircleDot { size: 9, fill: "currentColor" }
                }
                span { class: "sr-only", "Theme" }
                select {
                    class: "cursor-pointer appearance-none bg-transparent text-inherit outline-none disabled:cursor-not-allowed",
                    value: theme_value(theme),
                    disabled: shell.is_none() || viewer.render_command_pending(),
                    aria_label: "Theme",
                    onchange: move |event| {
                        if let Some(theme) = theme_from_value(&event.value()) {
                            viewer.set_preference(SetViewerPreference::Theme(theme));
                        }
                    },
                    for option in [
                        ViewerTheme::Dark,
                        ViewerTheme::Light,
                        ViewerTheme::Hearth,
                        ViewerTheme::Mirage,
                        ViewerTheme::Glacier,
                        ViewerTheme::Noir,
                        ViewerTheme::Graphite,
                    ]
                    {
                        option { value: theme_value(option), "{theme_label(option)}" }
                    }
                }
            }

            Link {
                class: if matches!(route, Route::Settings {}) { NAVIGATION_ACTION_ACTIVE_CLASSES } else { NAVIGATION_ACTION_CLASSES },
                to: Route::Settings {},
                aria_current: matches!(route, Route::Settings {}).then_some("page"),
                aria_label: "User settings",
                title: "User settings",
                span { aria_hidden: "true",
                    Settings { size: 15 }
                }
            }
        }
        if let Some(error) = action_error() {
            FloatingNotice { state: FloatingNoticeState::Error, role: "alert", "{error.message()}" }
        }
    }
}

const fn active_tab_id(active: &ViewerActiveState) -> Option<u64> {
    match active {
        ViewerActiveState::Empty => None,
        ViewerActiveState::Pending { tab_id }
        | ViewerActiveState::Broken { tab_id, .. }
        | ViewerActiveState::Error { tab_id, .. } => Some(*tab_id),
        ViewerActiveState::Ready { view } => Some(view.identity.tab_id),
    }
}

fn tab_element_id(tab_id: u64) -> String {
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

fn tab_focus_target(ids: &[u64], current: u64, movement: TabMovement) -> Option<u64> {
    let current_index = ids.iter().position(|id| *id == current)?;
    let target_index = match movement {
        TabMovement::Next => (current_index + 1) % ids.len(),
        TabMovement::Previous => current_index.checked_sub(1).unwrap_or(ids.len() - 1),
        TabMovement::First => 0,
        TabMovement::Last => ids.len() - 1,
    };
    ids.get(target_index).copied()
}

fn close_focus_target(tabs: &[ViewerTab], closing: u64) -> Option<u64> {
    let index = tabs.iter().position(|tab| tab.id == closing)?;
    tabs.get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|previous| tabs.get(previous)))
        .map(|tab| tab.id)
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{ViewerTab, ViewerTabKind, ViewerTabState};

    use super::{TabMovement, close_focus_target, tab_focus_target, tab_state_label};

    #[test]
    fn tab_focus_wraps_and_supports_edges() {
        let ids = [4, 8, 15];

        assert_eq!(tab_focus_target(&ids, 15, TabMovement::Next), Some(4));
        assert_eq!(tab_focus_target(&ids, 4, TabMovement::Previous), Some(15));
        assert_eq!(tab_focus_target(&ids, 8, TabMovement::First), Some(4));
        assert_eq!(tab_focus_target(&ids, 8, TabMovement::Last), Some(15));
    }

    #[test]
    fn final_tab_close_targets_the_workspace_heading() {
        let tabs = [ViewerTab {
            id: 4,
            label: "Only diff".to_owned(),
            kind: ViewerTabKind::Snapshot,
            state: ViewerTabState::Ready,
        }];

        assert_eq!(close_focus_target(&tabs, 4), None);
    }

    #[test]
    fn every_tab_state_has_a_non_color_label() {
        assert_eq!(tab_state_label(&ViewerTabState::Ready), "Ready");
        assert_eq!(tab_state_label(&ViewerTabState::Pending), "Rendering");
        assert_eq!(tab_state_label(&ViewerTabState::Broken), "Render stopped");
        assert_eq!(tab_state_label(&ViewerTabState::Error), "Render failed");
    }
}
