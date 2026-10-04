use dioxus::prelude::*;
use gtl_models::diffs::{ExtensionFilter, ExtensionFilterMode};
use lucide_dioxus::{ListFilter, RotateCcw, Search};

use crate::{
    shared::{
        i18n::{t, use_language},
        ui::{
            Button, ButtonLayout, ButtonSize, ButtonVariant, Checkbox, ExtensionSelectionAction,
            ExtensionSelectionInput, FieldLabelVisibility, IconPopover, Select, SelectOption,
            TextInput, popover::PopoverPlacement, select::SelectVariant,
        },
    },
    views::diffs::{
        changes_since::{ChangesSinceEdit, ChangesSincePreset, ChangesSinceSelection},
        file_filter_form::{FileChangeKind, ShownFileChanges},
    },
};

pub(super) mod desktop;
pub(crate) mod workspace;

/// Groups every file filter of a diff behind the Files panel's filter button.
#[component]
pub(crate) fn FileFiltersMenu(
    id: String,
    text: String,
    text_status: String,
    #[props(default)] text_maxlength: Option<String>,
    ontext: EventHandler<String>,
    shown: ShownFileChanges,
    ontoggle: EventHandler<FileChangeKind>,
    #[props(default)] unreviewed: bool,
    #[props(default)] onunreviewed: EventHandler<bool>,
    changes_since: ChangesSinceSelection,
    #[props(default)] changes_since_value: String,
    #[props(default = true)] changes_since_available: bool,
    onchangessince: EventHandler<ChangesSinceEdit>,
    extension_filter: ExtensionFilter,
    extensions: Vec<String>,
    onextension: EventHandler<ExtensionFilter>,
    hidden_count: usize,
    active: bool,
    onclear: EventHandler<()>,
    children: Element,
) -> Element {
    let language = use_language();
    let label = if hidden_count > 0 {
        t!(language, "file-filters-label-hidden", count = hidden_count)
    } else {
        t!(language, "file-filters-label")
    };
    rsx! {
        IconPopover {
            id: id.clone(),
            aria_label: label,
            placement: PopoverPlacement::TriggerStart,
            icon: rsx! {
                span { class: "file-filters-trigger",
                    ListFilter { size: 16 }
                    if active {
                        span { class: "file-filters-active-dot" }
                    }
                }
            },
            div { class: "file-filters",
                h2 { class: "text-sm font-semibold", {t!(language, "file-filters-label")} }
                TextFilterSection {
                    id: format!("{id}-text"),
                    text,
                    status: text_status,
                    maxlength: text_maxlength,
                    ontext,
                }
                ChangeKindSection { shown, ontoggle }
                section { class: "file-filters-section",
                    Checkbox {
                        id: format!("{id}-unreviewed"),
                        label: t!(language, "review-filter-unreviewed"),
                        checked: unreviewed,
                        onchange: move |checked| onunreviewed.call(checked),
                    }
                }
                ChangesSinceSection {
                    id: format!("{id}-since"),
                    selection: changes_since,
                    value: changes_since_value,
                    available: changes_since_available,
                    onchange: onchangessince,
                }
                ExtensionFilterSection {
                    id: format!("{id}-extensions"),
                    filter: extension_filter,
                    available: extensions,
                    onchange: onextension,
                }
                {children}
                div { class: "file-filters-footer",
                    if hidden_count > 0 {
                        p { class: "text-xs",
                            {t!(language, "file-filters-hidden-count", count = hidden_count)}
                        }
                    }
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        layout: ButtonLayout::FullWidthStart,
                        disabled: !active,
                        onclick: move |_| onclear.call(()),
                        RotateCcw { size: 15 }
                        {t!(language, "file-filters-clear")}
                    }
                }
            }
        }
    }
}

#[component]
fn TextFilterSection(
    id: String,
    text: String,
    status: String,
    maxlength: Option<String>,
    ontext: EventHandler<String>,
) -> Element {
    let language = use_language();
    let status_id = format!("{id}-status");
    rsx! {
        section { class: "file-filters-section",
            h3 { class: "file-filters-heading", {t!(language, "file-filters-text")} }
            div { class: "relative min-w-0",
                span { class: "file-filters-search-icon", aria_hidden: "true",
                    Search { size: 15 }
                }
                TextInput {
                    id,
                    label: t!(language, "file-filters-text"),
                    label_visibility: FieldLabelVisibility::Hidden,
                    class: "h-9 py-2 pr-2 pl-8",
                    r#type: "search",
                    value: text,
                    maxlength,
                    placeholder: t!(language, "file-filters-text-placeholder"),
                    aria_describedby: status_id.clone(),
                    oninput: move |event: FormEvent| ontext.call(event.value()),
                }
            }
            p {
                id: status_id,
                class: "min-h-4 text-xs text-ink-3",
                role: "status",
                aria_live: "polite",
                "{status}"
            }
        }
    }
}

#[component]
fn ChangeKindSection(shown: ShownFileChanges, ontoggle: EventHandler<FileChangeKind>) -> Element {
    let language = use_language();
    let heading = t!(language, "file-filters-changes");
    rsx! {
        section { class: "file-filters-section",
            h3 { class: "file-filters-heading", "{heading}" }
            div {
                class: "file-filters-segments",
                role: "group",
                aria_label: heading.clone(),
                for kind in FileChangeKind::ALL {
                    Button {
                        key: "{kind.as_str()}",
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Toggle,
                        aria_pressed: shown.shows(kind).to_string(),
                        title: change_kind_hint(kind, language),
                        onclick: move |_| ontoggle.call(kind),
                        {change_kind_label(kind, language)}
                    }
                }
            }
        }
    }
}

fn change_kind_label(
    kind: FileChangeKind,
    language: gtl_models::settings::ViewerLanguage,
) -> String {
    match kind {
        FileChangeKind::Added => t!(language, "file-filters-change-added"),
        FileChangeKind::Removed => t!(language, "file-filters-change-removed"),
        FileChangeKind::Modified => t!(language, "file-filters-change-modified"),
    }
}

fn change_kind_hint(
    kind: FileChangeKind,
    language: gtl_models::settings::ViewerLanguage,
) -> Option<String> {
    (kind == FileChangeKind::Modified).then(|| t!(language, "file-filters-change-modified-hint"))
}

#[component]
fn ChangesSinceSection(
    id: String,
    selection: ChangesSinceSelection,
    value: String,
    available: bool,
    onchange: EventHandler<ChangesSinceEdit>,
) -> Element {
    let language = use_language();
    let heading = t!(language, "file-filters-since");
    let options = [SelectOption::new(
        ChangesSinceSelection::AnyTime.value(),
        t!(language, "file-filters-since-any-time"),
    )]
    .into_iter()
    .chain(ChangesSincePreset::ALL.map(|preset| {
        SelectOption::new(
            preset.as_str(),
            changes_since_preset_label(preset, language),
        )
    }))
    .chain([SelectOption::new(
        ChangesSinceSelection::Custom.value(),
        t!(language, "file-filters-since-custom"),
    )])
    .collect::<Vec<_>>();
    let description = if available {
        t!(language, "file-filters-since-description")
    } else {
        t!(language, "file-filters-since-unavailable")
    };
    rsx! {
        section { class: "file-filters-section",
            h3 { class: "file-filters-heading", "{heading}" }
            Select {
                id: format!("{id}-range"),
                aria_label: heading.clone(),
                value: selection.value(),
                options,
                variant: SelectVariant::Toolbar,
                disabled: !available,
                onchange: move |value: String| {
                    onchange.call(ChangesSinceEdit::from_selection_value(&value));
                },
            }
            if selection == ChangesSinceSelection::Custom {
                TextInput {
                    id: format!("{id}-custom"),
                    label: t!(language, "file-filters-since-custom-label"),
                    label_visibility: FieldLabelVisibility::Hidden,
                    r#type: "datetime-local",
                    value,
                    disabled: !available,
                    onchange: move |event: FormEvent| {
                        onchange.call(ChangesSinceEdit::CustomValue(event.value()));
                    },
                }
            }
            p { class: "text-xs text-ink-3", "{description}" }
        }
    }
}

fn changes_since_preset_label(
    preset: ChangesSincePreset,
    language: gtl_models::settings::ViewerLanguage,
) -> String {
    match preset {
        ChangesSincePreset::LastHour => t!(language, "file-filters-since-last-hour"),
        ChangesSincePreset::Today => t!(language, "file-filters-since-today"),
        ChangesSincePreset::Last24Hours => t!(language, "file-filters-since-last-24-hours"),
        ChangesSincePreset::Last7Days => t!(language, "file-filters-since-last-7-days"),
    }
}

#[component]
fn ExtensionFilterSection(
    id: String,
    filter: ExtensionFilter,
    available: Vec<String>,
    onchange: EventHandler<ExtensionFilter>,
) -> Element {
    let language = use_language();
    let mode = filter.mode();
    let extensions = filter.extensions().clone();
    let description = match mode {
        ExtensionFilterMode::Only => t!(language, "extensions-mode-only-description"),
        ExtensionFilterMode::Hide => t!(language, "extensions-mode-hide-description"),
    };
    rsx! {
        section { class: "file-filters-section",
            div { class: "file-filters-section-header",
                h3 { class: "file-filters-heading", {t!(language, "file-filters-extensions")} }
                ExtensionFilterModeToggle {
                    mode,
                    onchange: {
                        let extensions = extensions.clone();
                        move |mode| onchange.call(ExtensionFilter::new(mode, extensions.clone()))
                    },
                }
            }
            p { class: "text-xs text-ink-3", "{description}" }
            ExtensionSelectionInput {
                id,
                label: t!(language, "extensions-selected"),
                selected: extensions.clone(),
                available,
                onchange: move |action: ExtensionSelectionAction| {
                    onchange.call(ExtensionFilter::new(mode, action.apply(&extensions)));
                },
            }
        }
    }
}

#[component]
fn ExtensionFilterModeToggle(
    mode: ExtensionFilterMode,
    onchange: EventHandler<ExtensionFilterMode>,
) -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "file-filters-segments",
            role: "group",
            aria_label: t!(language, "extensions-mode-label"),
            for (value, label) in [
                (ExtensionFilterMode::Only, t!(language, "extensions-mode-only")),
                (ExtensionFilterMode::Hide, t!(language, "extensions-mode-hide")),
            ]
            {
                Button {
                    key: "{value}",
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Toggle,
                    aria_pressed: (mode == value).to_string(),
                    onclick: move |_| {
                        if mode != value {
                            onchange.call(value);
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use gtl_models::diffs::{ExtensionFilter, ExtensionFilterMode, FileExtensions};

    use super::FileFiltersMenu;
    use crate::views::diffs::{
        changes_since::ChangesSinceSelection, file_filter_form::ShownFileChanges,
    };

    #[component]
    fn MenuTestView() -> Element {
        rsx! {
            FileFiltersMenu {
                id: "filters",
                text: "needle",
                text_status: "8 matches",
                ontext: move |_| {},
                shown: ShownFileChanges::default(),
                ontoggle: move |_| {},
                changes_since: ChangesSinceSelection::Custom,
                changes_since_value: "2026-09-27T17:00",
                onchangessince: move |_| {},
                extension_filter: ExtensionFilter::new(ExtensionFilterMode::Hide, FileExtensions::new(["lock"])),
                extensions: vec!["lock".to_owned(), "rs".to_owned()],
                onextension: move |_| {},
                hidden_count: 3,
                active: true,
                onclear: move |()| {},
            }
        }
    }

    #[test]
    fn trigger_label_counts_the_hidden_files() {
        let html = dioxus_ssr::render_element(rsx! {
            MenuTestView {}
        });

        assert!(html.contains("Filter files: 3 files hidden"));
        assert!(html.contains("8 matches"));
        assert!(html.contains("value=\"2026-09-27T17:00\""));
    }
}
