use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerDiffFileId, ViewerFileSummary};
use lucide_dioxus::{File, ListFilter};

use super::{DiffWorkspaceContext, file_search::WorkspaceFileMatches, use_workspace_context};
use crate::shared::ui::{
    Button, ButtonSize, ButtonVariant, ScrollArea, SearchPanel, SearchPanelPlacement, TextInput,
    TextInputLabelVisibility,
};

const PATH_FILTER_INPUT_ID: &str = "viewer-path-filter";
const PATH_FILTER_LABEL: &str = "Filter files by path";

pub(super) fn open_path_filter(mut workspace: DiffWorkspaceContext) {
    workspace.path_filter_open.set(true);
    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    crate::shared::browser::focus_element(PATH_FILTER_INPUT_ID.to_owned());
}

fn close_path_filter(mut workspace: DiffWorkspaceContext) {
    workspace.path_filter_open.set(false);
    #[cfg(any(feature = "component-preview", feature = "desktop"))]
    crate::shared::browser::focus_element("workspace-heading".to_owned());
}

#[component]
pub(super) fn PathFilterTrigger(artifact_view_id: Option<String>) -> Element {
    let workspace = use_workspace_context();
    rsx! {
        Button {
            class: "mobile:size-11",
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: PATH_FILTER_LABEL,
            title: PATH_FILTER_LABEL,
            aria_expanded: (workspace.path_filter_open)().to_string(),
            "data-gtl-action": artifact_view_id.map(|_| "open-path-filter"),
            onclick: move |_| open_path_filter(workspace),
            span { aria_hidden: "true",
                ListFilter { size: 16 }
            }
        }
    }
}

#[component]
pub(super) fn PathFilter(
    onnavigate: EventHandler<String>,
    artifact_view_id: Option<String>,
) -> Element {
    let mut workspace = use_workspace_context();
    let mut selected = use_signal(|| None::<ViewerDiffFileId>);
    let artifact = artifact_view_id.is_some();
    let input_id = artifact_view_id.map_or_else(
        || PATH_FILTER_INPUT_ID.to_owned(),
        |id| format!("artifact-view-{id}-path-filter"),
    );
    let results_id = format!("{input_id}-results");
    let open = (workspace.path_filter_open)();
    let onselect = use_callback(move |file: ViewerFileSummary| {
        close_path_filter(workspace);
        onnavigate.call(file.anchor_id);
    });
    #[cfg(any(feature = "component-preview", feature = "desktop"))]
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
    if !open && !artifact {
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
            label: PATH_FILTER_LABEL,
            placement: SearchPanelPlacement::WorkspaceCenter,
            hidden: !open,
            "data-gtl-path-filter": artifact.then_some(""),
            onfocusout: move |_| workspace.path_filter_open.set(false),
            onkeydown: move |event| handle_keydown(&event, &workspace, selected, onselect),
            div { class: "p-2",
                TextInput {
                    id: input_id,
                    label: PATH_FILTER_LABEL,
                    label_visibility: TextInputLabelVisibility::Hidden,
                    value: (workspace.file_filter)(),
                    placeholder: PATH_FILTER_LABEL,
                    role: "combobox",
                    aria_autocomplete: "list",
                    aria_expanded: open.to_string(),
                    aria_controls: results_id.clone(),
                    aria_activedescendant: active_id,
                    "data-gtl-action": artifact.then_some("filter-files"),
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
                aria_label: "Matching files",
                for file in files {
                    PathFilterOption {
                        key: "{file.id.as_str()}",
                        id: format!("{results_id}-{}", file.id.as_str()),
                        file: file.clone(),
                        active: active.as_ref() == Some(&file.id),
                        artifact,
                        onselect,
                    }
                }
            }
            if let Some(message) = (!artifact).then(|| matches_message(&matches)).flatten() {
                p { class: "px-3 pb-3 text-xs text-ink-3", role: "status", "{message}" }
            }
            if artifact {
                p {
                    class: "px-3 pb-3 text-xs text-ink-3",
                    hidden: !files.is_empty(),
                    role: "status",
                    "data-gtl-path-filter-empty": "",
                    "No files match"
                }
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

fn matches_message(matches: &WorkspaceFileMatches) -> Option<&str> {
    match matches {
        WorkspaceFileMatches::Ready(files) => files.is_empty().then_some("No files match"),
        #[cfg(feature = "desktop")]
        WorkspaceFileMatches::Loading => Some("Searching files..."),
        #[cfg(feature = "desktop")]
        WorkspaceFileMatches::Error(message) => Some(message),
    }
}

#[component]
fn PathFilterOption(
    id: String,
    file: ViewerFileSummary,
    active: bool,
    artifact: bool,
    onselect: EventHandler<ViewerFileSummary>,
) -> Element {
    let path = file.path.to_string_lossy().into_owned();
    let (directory, name) = path.rsplit_once('/').unwrap_or(("", &path));
    rsx! {
        div {
            id,
            class: "flex min-h-8 cursor-pointer items-center gap-2 rounded-sm px-2 py-1 text-sm text-ink-2 hover:bg-surface-2 aria-selected:bg-acc-soft aria-selected:text-ink mobile:min-h-11",
            role: "option",
            aria_label: path.clone(),
            aria_selected: active.to_string(),
            title: path.clone(),
            "data-file-target": file.anchor_id.clone(),
            "data-gtl-filter-key": artifact.then(|| path.to_lowercase()),
            "data-gtl-action": artifact.then_some("select-path-filter-file"),
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
