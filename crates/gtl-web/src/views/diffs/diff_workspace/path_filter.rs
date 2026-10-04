use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::{ViewerDiffFileId, ViewerFileSummary};
use lucide_dioxus::File;

use super::{DiffWorkspaceContext, file_search::WorkspaceFileMatches, use_workspace_context};
use crate::shared::{
    i18n::{t, use_language},
    ui::{FieldLabelVisibility, ScrollArea, SearchPanel, SearchPanelPlacement, TextInput},
};

const PATH_FILTER_INPUT_ID: &str = "viewer-path-filter";

pub(super) fn open_path_filter(mut workspace: DiffWorkspaceContext) {
    workspace.path_filter_open.set(true);
    crate::shared::browser::focus_element(PATH_FILTER_INPUT_ID.to_owned());
}

fn close_path_filter(mut workspace: DiffWorkspaceContext) {
    workspace.path_filter_open.set(false);
    crate::shared::browser::focus_element("workspace-heading".to_owned());
}

#[component]
pub(super) fn PathFilter(onnavigate: EventHandler<String>) -> Element {
    let language = use_language();
    let mut workspace = use_workspace_context();
    let mut selected = use_signal(|| None::<ViewerDiffFileId>);
    let input_id = PATH_FILTER_INPUT_ID.to_owned();
    let results_id = format!("{input_id}-results");
    let open = (workspace.path_filter_open)();
    let onselect = use_callback(move |file: ViewerFileSummary| {
        close_path_filter(workspace);
        onnavigate.call(file.anchor_id);
    });
    {
        let focus_input_id = input_id.clone();
        use_effect(move || {
            if (workspace.path_filter_open)() {
                crate::shared::browser::focus_element(focus_input_id.clone());
            }
        });
        let results_id = results_id.clone();
        use_effect(move || {
            if (workspace.path_filter_open)()
                && let Some(file) = selected_file(
                    workspace.file_matches.read().files(),
                    selected.read().as_ref(),
                )
            {
                crate::shared::browser::scroll_option_into_view(&format!(
                    "{results_id}-{}",
                    file.id.as_str()
                ));
            }
        });
    }
    if !open {
        return rsx! {};
    }
    let matches = workspace.file_matches.read();
    let files = matches.files();
    let active = selected_file(files, selected.read().as_ref()).map(|file| file.id.clone());
    let active_id = active
        .as_ref()
        .map(|id| format!("{results_id}-{}", id.as_str()));
    rsx! {
        SearchPanel {
            label: t!(language, "path-filter-label"),
            placement: SearchPanelPlacement::WorkspaceCenter,
            hidden: !open,

            onfocusout: move |_| workspace.path_filter_open.set(false),
            onkeydown: move |event| handle_keydown(&event, &workspace, selected, onselect),
            div { class: "p-2",
                TextInput {
                    id: input_id,
                    label: t!(language, "path-filter-label"),
                    label_visibility: FieldLabelVisibility::Hidden,
                    value: (workspace.file_filter)(),
                    placeholder: t!(language, "path-filter-label"),
                    role: "combobox",
                    aria_autocomplete: "list",
                    aria_expanded: open.to_string(),
                    aria_controls: results_id.clone(),
                    aria_activedescendant: active_id,

                    oninput: move |event: FormEvent| {
                        selected.set(None);
                        workspace.file_filter.set(event.value());
                    },
                }
            }
            ScrollArea {
                id: results_id.clone(),
                class: "relative max-h-[min(24rem,50vh)] overflow-y-auto px-1 pb-1",
                role: "listbox",
                aria_label: t!(language, "path-filter-results"),
                for file in files {
                    PathFilterOption {
                        key: "{file.id.as_str()}",
                        id: format!("{results_id}-{}", file.id.as_str()),
                        file: file.clone(),
                        active: active.as_ref() == Some(&file.id),

                        onselect,
                    }
                }
            }
            if let Some(message) = matches_message(&matches, language) {
                p { class: "px-3 pb-3 text-xs text-ink-3", role: "status", "{message}" }
            }

        }
    }
}

fn handle_keydown(
    event: &KeyboardEvent,
    workspace: &DiffWorkspaceContext,
    mut selected: Signal<Option<ViewerDiffFileId>>,
    onselect: Callback<ViewerFileSummary>,
) {
    if event.is_composing() {
        return;
    }
    let matches = workspace.file_matches.read();
    let files = matches.files();
    match event.key() {
        Key::Escape => close_path_filter(*workspace),
        Key::ArrowDown | Key::ArrowUp => {
            if files.is_empty() {
                event.prevent_default();
                return;
            }
            let index = selected_file_index(files, selected.peek().as_ref());
            let index = if event.key() == Key::ArrowDown {
                (index + 1) % files.len()
            } else {
                (index + files.len() - 1) % files.len()
            };
            selected.set(Some(files[index].id.clone()));
        }
        Key::Enter => {
            if let Some(file) = selected_file(files, selected.peek().as_ref()) {
                onselect.call(file.clone());
            }
        }
        _ => return,
    }
    event.prevent_default();
    event.stop_propagation();
}

fn selected_file_index(files: &[ViewerFileSummary], selected: Option<&ViewerDiffFileId>) -> usize {
    files
        .iter()
        .position(|file| Some(&file.id) == selected)
        .unwrap_or(0)
}

fn selected_file<'a>(
    files: &'a [ViewerFileSummary],
    selected: Option<&ViewerDiffFileId>,
) -> Option<&'a ViewerFileSummary> {
    files.get(selected_file_index(files, selected))
}

fn matches_message(matches: &WorkspaceFileMatches, language: ViewerLanguage) -> Option<String> {
    match matches {
        WorkspaceFileMatches::Ready(files) => {
            files.is_empty().then(|| t!(language, "path-filter-empty"))
        }
        WorkspaceFileMatches::Loading => Some(t!(language, "path-filter-searching")),
        WorkspaceFileMatches::Error(error) => Some(
            crate::shared::failure_notice::client_error_message(error, language),
        ),
    }
}

#[component]
fn PathFilterOption(
    id: String,
    file: ViewerFileSummary,
    active: bool,

    onselect: EventHandler<ViewerFileSummary>,
) -> Element {
    let path = file.path.to_string_lossy().into_owned();
    let (directory, name) = path.rsplit_once('/').unwrap_or(("", &path));
    rsx! {
        div {
            id,
            class: "diff-path-filter-option min-h-8 gap-2 px-2 py-1 text-sm mobile:min-h-11",
            role: "option",
            aria_label: path.clone(),
            aria_selected: active.to_string(),
            title: path.clone(),
            "data-file-target": file.anchor_id.clone(),

            onmousedown: move |event| event.prevent_default(),
            onclick: move |_| onselect.call(file.clone()),
            span { class: "flex-none text-ink-3", aria_hidden: "true",
                File { size: 14 }
            }
            span { class: "min-w-0 max-w-[60%] shrink-0 truncate", "{name}" }
            span { class: "min-w-0 truncate text-xs text-ink-3", "{directory}" }
        }
    }
}
