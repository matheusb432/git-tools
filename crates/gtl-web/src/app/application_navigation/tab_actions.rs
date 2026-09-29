use dioxus::prelude::*;
use gtl_wire::viewer::ViewerActiveState;
use lucide_dioxus::{ChevronsDownUp, ChevronsUpDown, Upload};

use crate::{
    app::application_layout::{ViewerContext, ViewerShellLoad},
    entities::diffs::live_errors::LiveError,
    shared::{
        browser,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::viewer_tab::ViewerTabMenuTarget,
    },
    views::{diffs::presentation::DiffPresentation, push::availability::ViewPushMenuAction},
};

#[component]
pub(super) fn DiffTabActions(target: ViewerTabMenuTarget) -> Element {
    let language = use_language();
    let viewer = use_context::<ViewerContext>();
    let presentation = use_context::<DiffPresentation>();
    let tab_id = target.tab_id;
    let folded = use_memo(use_reactive((&tab_id,), move |(tab_id,)| {
        let _ = presentation.fold_command.read();
        presentation.all_folded(tab_id).unwrap_or(false)
    }));
    let shell = viewer.shell();
    let shell = shell.read();
    let ViewerShellLoad::Ready(shell) = &*shell else {
        return rsx! {};
    };
    let view = match &shell.active {
        ViewerActiveState::Ready { view } if view.identity.tab_id == tab_id => Some(view),
        _ => None,
    };
    let ready = shell
        .tabs
        .iter()
        .any(|tab| tab.id == tab_id && tab.state == gtl_wire::viewer::ViewerTabState::Ready);
    let fold_menu = target.menu_id.clone();
    let fold_trigger = target.trigger_id.clone();
    rsx! {
        if let Some(view) = view {
            ViewPushMenuAction {
                identity: view.identity,
                disabled: view.modified_files || view.commit_count == 0,
                menu_id: target.menu_id,
                trigger_id: target.trigger_id,
            }
        } else {
            button {
                class: "control-menu-action viewer-tab-context-action",
                r#type: "button",
                role: "menuitem",
                tabindex: "-1",
                disabled: true,
                title: t!(language, "tab-push-activate"),
                span { class: "inline-flex text-acc", aria_hidden: "true",
                    Upload { size: 14 }
                }
                {t!(language, "push-button")}
            }
        }
        button {
            class: "control-menu-action viewer-tab-context-action",
            r#type: "button",
            role: "menuitem",
            tabindex: "-1",
            disabled: !ready,
            onclick: move |_| {
                browser::hide_popover(&fold_menu);
                presentation.toggle_files(tab_id);
                browser::focus_element(fold_trigger.clone());
            },
            span { class: "inline-flex text-acc", aria_hidden: "true",
                if folded() {
                    ChevronsUpDown { size: 14 }
                } else {
                    ChevronsDownUp { size: 14 }
                }
            }
            if folded() {
                {t!(language, "files-expand-diffs")}
            } else {
                {t!(language, "files-collapse-diffs")}
            }
        }
    }
}

#[component]
pub(super) fn LiveWarningDetails(errors: Vec<LiveError>) -> Element {
    let language = use_language();
    rsx! {
        div { class: "mt-2 border-t border-line pt-2",
            p { class: "font-semibold text-warn", {t!(language, "workspace-live-recent-errors")} }
            ul { class: "mt-1 grid gap-2 text-xs",
                for (index, entry) in errors.iter().enumerate() {
                    li { key: "{index}", class: "break-words",
                        p { {client_error_message(&entry.error, language)} }
                        if let Some(diagnostic) = entry.error.diagnostic() {
                            p { class: "mt-0.5 font-mono text-ink-3 break-all", "{diagnostic}" }
                        }
                        if entry.occurrences > 1 {
                            p { class: "mt-0.5 text-ink-3",
                                {t!(language, "workspace-live-occurrences", count = entry.occurrences)}
                            }
                        }
                    }
                }
            }
        }
    }
}
