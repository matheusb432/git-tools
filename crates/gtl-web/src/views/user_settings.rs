pub mod ui;

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerRenderOptions, ViewerTheme, ViewerUserSettings};
use lucide_dioxus::{FileCog, Settings};

use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{Button, ButtonVariant, PageNotice, ScrollArea, Skeleton, use_toast},
        viewer_theme::viewer_theme_label,
    },
    views::{
        user_settings::ui::DiffExtensionExclusions,
        viewer_settings_form::{
            ViewerSettingsForm, ViewerSettingsSelection, viewer_settings_patch,
        },
    },
};

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
        SettingsEditableForm {
            configured_theme: settings.configured_theme,
            render_options: settings.render_options,
        }

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
fn SettingsEditableForm(
    configured_theme: Option<ViewerTheme>,
    render_options: ViewerRenderOptions,
) -> Element {
    let toast = use_toast();
    let initial = ViewerSettingsSelection::new(configured_theme, render_options);
    let mut persisted = use_signal(|| initial);
    let mut pending = use_signal(|| false);
    let mut saved = use_signal(|| false);

    rsx! {
        ViewerSettingsForm {
            initial,
            pending: pending(),
            saved: saved(),
            onmodified: move |()| saved.set(false),
            onsubmit: move |selected| {
                if pending() {
                    return;
                }
                saved.set(false);
                let request = viewer_settings_patch(persisted(), selected);
                pending.set(true);
                spawn(async move {
                    match viewer_server::edit_settings(request).await {
                        Ok(()) => {
                            persisted.set(selected);
                            saved.set(true);
                            toast.ok("Settings saved");
                        }
                        Err(error) => toast.error(error.message()),
                    }
                    pending.set(false);
                });
            },
        }
    }
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
