use dioxus::prelude::*;
use gtl_models::diffs::ExcludedExtensions;
use lucide_dioxus::{Check, Plus, Search, X};

use super::ExtensionExclusionsAction;
use crate::shared::{
    browser,
    file_extension::FileExtension,
    ui::{Button, ButtonLayout, ButtonSize, ButtonVariant, TextInput, TextInputLabelVisibility},
};

const SUGGESTIONS_MAX: usize = 12;

#[component]
pub(super) fn ExtensionSearch(
    input_id: String,
    results_id: String,
    add_id: String,
    available: Vec<String>,
    excluded: ExcludedExtensions,
    disabled: bool,
    onchange: EventHandler<ExtensionExclusionsAction>,
    onclose: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut active = use_signal(|| 0_usize);
    let focus_input_id = input_id.clone();
    use_effect(move || browser::focus_element(focus_input_id.clone()));
    let query_value = query();
    let matches = matching_extensions(&available, &excluded, &query_value);
    let create = FileExtension::parse(&query_value).ok().filter(|value| {
        !available.iter().any(|entry| entry == value.as_str())
            && !excluded
                .extensions()
                .iter()
                .any(|entry| entry == value.as_str())
    });
    let options = matches
        .iter()
        .take(SUGGESTIONS_MAX)
        .cloned()
        .map(ExtensionExclusionsAction::Toggle)
        .chain(create.clone().map(ExtensionExclusionsAction::Exclude))
        .collect::<Vec<_>>();
    let create_index = matches.len().min(SUGGESTIONS_MAX);
    let active_index = active().min(options.len().saturating_sub(1));
    let active_id = (!options.is_empty()).then(|| format!("{results_id}-{active_index}"));
    let error = if query_value.is_empty() || !matches.is_empty() || create.is_some() {
        None
    } else {
        FileExtension::parse(&query_value).err().map(str::to_owned)
    };
    let focus_selected_input_id = input_id.clone();
    let select = use_callback(move |action: ExtensionExclusionsAction| {
        if disabled {
            return;
        }
        if matches!(action, ExtensionExclusionsAction::Exclude(_)) {
            query.set(String::new());
            active.set(0);
        }
        onchange.call(action);
        browser::focus_element(focus_selected_input_id.clone());
    });
    rsx! {
        div {
            class: "extension-filter-search",
            onkeydown: move |event| {
                if disabled || event.is_composing() {
                    return;
                }
                match event.key() {
                    Key::ArrowDown if !options.is_empty() => {
                        active.set((active_index + 1) % options.len());
                    }
                    Key::ArrowUp if !options.is_empty() => {
                        active.set((active_index + options.len() - 1) % options.len());
                    }
                    Key::Enter => {
                        if let Some(option) = options.get(active_index) {
                            select.call(option.clone());
                        }
                    }
                    Key::Escape => {
                        onclose.call(());
                        browser::focus_element(add_id.clone());
                    }
                    _ => return,
                }
                event.prevent_default();
                event.stop_propagation();
            },
            span { class: "extension-filter-search-icon", aria_hidden: "true",
                Search { size: 16 }
            }
            TextInput {
                id: input_id.clone(),
                label: "Search or add an extension",
                label_visibility: TextInputLabelVisibility::Hidden,
                class: "pl-9 pr-8",
                placeholder: "Search or add…",
                value: query_value,
                maxlength: "255",
                autocomplete: "off",
                "spellcheck": "false",
                role: "combobox",
                aria_autocomplete: "list",
                aria_expanded: "true",
                aria_controls: results_id.clone(),
                aria_activedescendant: active_id,
                error,
                disabled,
                oninput: move |event: FormEvent| {
                    query.set(event.value());
                    active.set(0);
                },
            }
            if !query().is_empty() {
                Button {
                    class: "extension-filter-search-clear",
                    size: ButtonSize::IconCompact,
                    variant: ButtonVariant::Ghost,
                    aria_label: "Clear extension search",
                    disabled,
                    onclick: move |_| {
                        query.set(String::new());
                        active.set(0);
                        browser::focus_element(input_id.clone());
                    },
                    X { size: 14 }
                }
            }
        }
        div {
            id: results_id.clone(),
            role: "listbox",
            aria_label: "Extensions to exclude",
            aria_multiselectable: "true",
            class: "extension-filter-results",
            onmousedown: move |event| event.prevent_default(),
            if !matches.is_empty() {
                for (index, extension) in matches.iter().take(SUGGESTIONS_MAX).cloned().enumerate() {
                    Fragment { key: "{extension}",
                        if index == 0
                            || available.contains(&extension) != available.contains(&matches[index - 1])
                        {
                            p {
                                role: "presentation",
                                class: "extension-filter-group",
                                if available.contains(&extension) {
                                    "In this diff"
                                } else {
                                    "Other exclusions"
                                }
                            }
                        }
                        ExtensionOption {
                            results_id: results_id.clone(),
                            index,
                            extension: extension.clone(),
                            selected: excluded.extensions().contains(&extension),
                            active: index == active_index,
                            disabled,
                            onselect: move |()| {
                                select.call(ExtensionExclusionsAction::Toggle(extension.clone()));
                            },
                        }
                    }
                }
            }
            if let Some(extension) = create {
                CreateExtension {
                    results_id: results_id.clone(),
                    index: create_index,
                    extension,
                    active: create_index == active_index,
                    disabled,
                    onselect: move |extension| {
                        select.call(ExtensionExclusionsAction::Exclude(extension));
                    },
                }
            }
        }
        if matches.len() > SUGGESTIONS_MAX {
            p { class: "text-xs text-ink-3", "Search to find more extensions" }
        }
    }
}

#[component]
fn ExtensionOption(
    results_id: String,
    index: usize,
    extension: String,
    selected: bool,
    active: bool,
    disabled: bool,
    onselect: EventHandler<()>,
) -> Element {
    rsx! {
        Button {
            id: format!("{results_id}-{index}"),
            class: "extension-filter-option",
            size: ButtonSize::Medium,
            variant: ButtonVariant::Outline,
            role: "option",
            aria_selected: selected.to_string(),
            tabindex: "-1",
            "data-active": active.to_string(),
            aria_label: "Exclude .{extension}",
            disabled,
            onclick: move |_| onselect.call(()),
            code { class: "min-w-0 flex-1 truncate", ".{extension}" }
            if selected {
                Check { size: 15 }
            }
        }
    }
}

fn matching_extensions(
    available: &[String],
    excluded: &ExcludedExtensions,
    query: &str,
) -> Vec<String> {
    let query = query.trim().trim_start_matches('.').to_lowercase();
    available
        .iter()
        .chain(
            excluded
                .extensions()
                .iter()
                .filter(|entry| !available.contains(entry)),
        )
        .filter(|entry| entry.contains(&query))
        .cloned()
        .collect()
}

#[component]
fn CreateExtension(
    results_id: String,
    index: usize,
    extension: FileExtension,
    active: bool,
    disabled: bool,
    onselect: EventHandler<FileExtension>,
) -> Element {
    let label = format!("Add .{}", extension.as_str());
    rsx! {
        div { class: "col-span-2",
            Button {
                id: format!("{results_id}-{index}"),
                class: "extension-filter-create",
                size: ButtonSize::Medium,
                layout: ButtonLayout::FullWidthStart,
                variant: ButtonVariant::Outline,
                role: "option",
                aria_selected: "false",
                tabindex: "-1",
                "data-active": active.to_string(),
                disabled,
                onclick: move |_| onselect.call(extension.clone()),
                Plus { size: 16 }
                span { "{label}" }
            }
        }
    }
}
