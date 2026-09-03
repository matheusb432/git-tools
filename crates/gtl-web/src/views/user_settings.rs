pub mod ui;

use dioxus::prelude::*;
use gtl_wire::viewer::{
    EditSettingsRequest, FieldUpdate, ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions,
    ViewerUserSettings,
};
use lucide_dioxus::{Check, FileCog, Settings};

use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{
            Button, ButtonState, ButtonType, ButtonVariant, FieldLabel, PageNotice, ScrollArea,
            Select, SelectOption, Skeleton, use_toast,
        },
        viewer_theme::viewer_theme_label,
    },
    views::user_settings::ui::DiffExtensionExclusions,
};

const LAYOUT_OPTIONS: &[(&str, &str)] = &[
    ("", "Choose a layout"),
    ("unified", "Unified"),
    ("split", "Side by side"),
];
const DENSITY_OPTIONS: &[(&str, &str)] = &[
    ("", "Choose a view"),
    ("compact", "Changes only"),
    ("full", "Full file"),
];

#[component]
pub(crate) fn UserSettingsView() -> Element {
    let mut settings = use_resource(viewer_server::get_settings);

    use_effect(move || {
        browser::focus_element(gtl_web_contracts::user_settings::SETTINGS_HEADING_ID.into());
    });
    let pending = settings.state().cloned() == UseResourceState::Pending;
    let load = settings.read();

    rsx! {
        document::Title { "Settings - git-tools" }
        main { class: "h-full overflow-hidden bg-bg",
            ScrollArea { class: "h-full overflow-auto px-4 py-5 sm:px-6",
                div { class: "mx-auto grid max-w-5xl gap-5",
                    header { class: "border-b border-line pb-4",
                        div { class: "flex items-center gap-2 text-acc",
                            span { aria_hidden: "true",
                                Settings { size: 16 }
                            }
                            p { class: "font-semibold tracking-widest uppercase",
                                "Viewer preferences"
                            }
                        }
                        h1 {
                            id: gtl_web_contracts::user_settings::SETTINGS_HEADING_ID,
                            class: "mt-1 text-lg font-semibold tracking-tight text-ink focus:outline-none",
                            tabindex: "-1",
                            "User settings"
                        }
                        p { class: "mt-1 max-w-2xl leading-5 text-ink-2",
                            "Choose viewer defaults, then submit to save them."
                        }
                    }

                    match (pending, &*load) {
                        (true, _) | (false, None) => rsx! {
                            SettingsLoading {}
                        },
                        (false, Some(Err(error))) => {
                            let message = error.message();
                            rsx! {
                                PageNotice {
                                    class: "min-h-64",
                                    role: "alert",
                                    title: "Settings are unavailable",
                                    message,
                                    Button {
                                        class: "mx-auto mt-4",
                                        variant: ButtonVariant::Outline,
                                        onclick: move |_| settings.restart(),
                                        "Try again"
                                    }
                                }
                            }
                        }
                        (false, Some(Ok(settings))) => rsx! {
                            SettingsContent { settings: settings.clone() }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsLoading() -> Element {
    rsx! {
        div {
            class: "grid gap-1",
            role: "status",
            aria_label: "Loading settings",
            for _ in 0..7 {
                div { class: "grid gap-2 border-b border-line py-4 sm:grid-cols-[14rem_minmax(0,1fr)]",
                    Skeleton { class: "h-4 w-3/5" }
                    Skeleton { class: "h-4 w-4/5" }
                }
            }
            span { class: "sr-only", "Loading settings" }
        }
    }
}

#[component]
fn SettingsContent(settings: ViewerUserSettings) -> Element {
    let mut projects = settings.diff_exclusions.projects.clone();
    projects.sort_by(|left, right| left.project_name.cmp(&right.project_name));
    let configuration_path = settings
        .configuration_path
        .as_deref()
        .unwrap_or("Built-in defaults");
    let push_confirmation_text = if settings.push_confirmation_required {
        "Required"
    } else {
        "Not required"
    };

    rsx! {
        SettingsEditableForm { render_options: settings.render_options }

        section {
            class: "overflow-hidden rounded-panel border border-line bg-surface",
            aria_label: "Resolved viewer settings",
            SettingsTableHeader {
                icon: rsx! {
                    FileCog {}
                },
                subtitle: "Current sources and effective values.",
                "Resolved configuration"
            }
            dl { class: "divide-y divide-line",
                SettingsRow { term: "Configuration file", "{configuration_path}" }
                SettingsRow { term: "Effective theme", "{viewer_theme_label(settings.effective_theme)}" }
                SettingsRow { term: "Push confirmation", "{push_confirmation_text}" }
                SettingsRow { term: "Default diff exclusions",
                    DiffExtensionExclusions { file_extensions: settings.diff_exclusions.default_extensions }
                }
            }
        }

        section {
            class: "overflow-hidden rounded-panel border border-line bg-surface",
            aria_label: "Project diff exclusions",
            SettingsTableHeader { subtitle: "Repository-specific extension filters.", "Project exclusions" }
            if projects.is_empty() {
                p { class: "px-4 py-5 text-ink-3", "No project-specific exclusions." }
            } else {
                for project in projects {
                    SettingsRow { term: project.project_name,
                        DiffExtensionExclusions { file_extensions: project.extensions }
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsEditableForm(render_options: ViewerRenderOptions) -> Element {
    let toast = use_toast();
    let mut layout = use_signal(|| render_options.layout.as_str().to_owned());
    let mut density = use_signal(|| render_options.density.as_str().to_owned());
    let mut saved_render_options = use_signal(|| render_options);
    let mut submission_attempted = use_signal(|| false);
    let mut pending = use_signal(|| false);
    let mut saved = use_signal(|| false);
    let layout_value = layout();
    let density_value = density();
    let layout_error = layout_error(submission_attempted(), &layout_value);
    let density_error = density_error(submission_attempted(), &density_value);

    rsx! {
        form {
            novalidate: true,
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                if pending() {
                    return;
                }
                submission_attempted.set(true);
                saved.set(false);
                let Ok((request, selected_render_options)) = settings_patch(
                    saved_render_options(),
                    &layout(),
                    &density(),
                ) else {
                    return;
                };
                pending.set(true);
                spawn(async move {
                    match viewer_server::edit_settings(request).await {
                        Ok(()) => {
                            saved_render_options.set(selected_render_options);
                            saved.set(true);
                            toast.ok("Settings saved");
                        }
                        Err(error) => toast.error(error.message()),
                    }
                    pending.set(false);
                });
            },
            section {
                class: "overflow-hidden rounded-panel border border-line bg-surface",
                aria_label: "Editable viewer settings",
                header { class: "border-b border-line bg-surface-2 px-4 py-3",
                    h2 { class: "font-semibold text-ink", "Diff display" }
                    p { class: "mt-0.5 text-xs leading-5 text-ink-3",
                        "These defaults apply to viewer sessions and raw artifacts."
                    }
                }
                div { class: "flex flex-col gap-4 p-4",
                    div { class: "flex flex-col gap-1.5",
                        FieldLabel {
                            for_id: "settings-layout",
                            label: "Layout",
                            hint: "Choose how old and new lines share the diff canvas.",
                        }
                        Select {
                            id: "settings-layout",
                            name: "layout",
                            aria_label: "Layout",
                            value: layout_value,
                            options: select_options(LAYOUT_OPTIONS),
                            error: layout_error,
                            disabled: pending(),
                            required: true,
                            onchange: move |event: FormEvent| {
                                layout.set(event.value());
                                saved.set(false);
                            },
                        }
                    }
                    div { class: "flex flex-col gap-1.5",
                        FieldLabel {
                            for_id: "settings-density",
                            label: "View",
                            hint: "Limit the document to changed regions or show the full file.",
                        }
                        Select {
                            id: "settings-density",
                            name: "density",
                            aria_label: "View",
                            value: density_value,
                            options: select_options(DENSITY_OPTIONS),
                            error: density_error,
                            disabled: pending(),
                            required: true,
                            onchange: move |event: FormEvent| {
                                density.set(event.value());
                                saved.set(false);
                            },
                        }
                    }
                }
                footer { class: "flex min-h-16 items-center justify-between gap-3 border-t border-line bg-surface-2 px-4 py-3",
                    p {
                        class: "text-xs text-add",
                        role: "status",
                        aria_live: "polite",
                        if saved() {
                            span { class: "inline-flex items-center gap-1.5",
                                Check { size: 14 }
                                "Settings saved"
                            }
                        }
                    }
                    Button {
                        button_type: ButtonType::Submit,
                        state: if pending() { ButtonState::Loading } else { ButtonState::Enabled },
                        "Save settings"
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsValidationError {
    Layout,
    Density,
}

fn settings_patch(
    current: ViewerRenderOptions,
    layout: &str,
    density: &str,
) -> Result<(EditSettingsRequest, ViewerRenderOptions), SettingsValidationError> {
    let layout = parse_layout(layout).ok_or(SettingsValidationError::Layout)?;
    let density = parse_density(density).ok_or(SettingsValidationError::Density)?;
    let selected = ViewerRenderOptions { layout, density };
    let request = EditSettingsRequest {
        layout: changed_field(&current.layout, selected.layout),
        density: changed_field(&current.density, selected.density),
        ..EditSettingsRequest::default()
    };
    Ok((request, selected))
}

fn parse_layout(value: &str) -> Option<ViewerDiffLayout> {
    match value {
        "unified" => Some(ViewerDiffLayout::Unified),
        "split" => Some(ViewerDiffLayout::Split),
        _ => None,
    }
}

fn parse_density(value: &str) -> Option<ViewerDiffDensity> {
    match value {
        "compact" => Some(ViewerDiffDensity::Compact),
        "full" => Some(ViewerDiffDensity::Full),
        _ => None,
    }
}

fn changed_field<T: PartialEq>(current: &T, selected: T) -> FieldUpdate<T> {
    if current == &selected {
        FieldUpdate::Unchanged
    } else {
        FieldUpdate::Update(selected)
    }
}

fn layout_error(attempted: bool, value: &str) -> Option<String> {
    (attempted && parse_layout(value).is_none()).then(|| "Choose a diff layout.".to_owned())
}

fn density_error(attempted: bool, value: &str) -> Option<String> {
    (attempted && parse_density(value).is_none()).then(|| "Choose a diff view.".to_owned())
}

fn select_options(options: &[(&str, &str)]) -> Vec<SelectOption> {
    options
        .iter()
        .map(|(value, label)| SelectOption::new(*value, *label))
        .collect()
}

#[component]
fn SettingsTableHeader(
    icon: Option<Element>,
    subtitle: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        header { class: "border-b border-line bg-surface-2 px-4 py-3",
            div { class: "flex items-center",
                if let Some(icon) = icon {
                    span { class: "mr-2 text-acc", aria_hidden: "true", {icon} }
                }
                h2 { class: "font-semibold text-ink", {children} }
            }
            if let Some(subtitle) = subtitle {
                p { class: "mt-0.5 text-xs text-ink-3", "{subtitle}" }
            }
        }
    }
}

#[component]
fn SettingsRow(term: String, children: Element) -> Element {
    rsx! {
        div { class: "grid gap-2 px-4 py-3 sm:grid-cols-[14rem_minmax(0,1fr)]",
            dt { class: "font-semibold text-ink-2", "{term}" }
            dd { class: "m-0 min-w-0 break-words text-ink", {children} }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::{FieldUpdate, ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions};

    use super::{SettingsValidationError, settings_patch};
    use crate::test_support::TestResult;

    #[test]
    fn settings_patch_updates_only_changed_fields() -> TestResult {
        let result = settings_patch(
            ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
            "split",
            "compact",
        );
        let Ok((request, selected)) = result else {
            return Err(std::io::Error::other("valid settings were rejected").into());
        };

        assert_eq!(request.layout, FieldUpdate::Update(ViewerDiffLayout::Split));
        assert_eq!(request.density, FieldUpdate::Unchanged);
        assert_eq!(request.theme, FieldUpdate::Unchanged);
        assert_eq!(selected.layout, ViewerDiffLayout::Split);
        Ok(())
    }

    #[test]
    fn settings_patch_rejects_unknown_control_values() {
        let current = ViewerRenderOptions {
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        };

        assert_eq!(
            settings_patch(current, "", "compact"),
            Err(SettingsValidationError::Layout)
        );
        assert_eq!(
            settings_patch(current, "unified", "dense"),
            Err(SettingsValidationError::Density)
        );
    }
}
