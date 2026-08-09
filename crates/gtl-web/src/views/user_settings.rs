use dioxus::prelude::*;
use gtl_contracts::viewer::ViewerUserSettings;
use lucide_dioxus::{FileCog, Settings};

use crate::{
    entities::diffs::{ViewerApi, density_label, layout_label, theme_label},
    shared::{
        bridge::ClientApiError,
        browser,
        ui::{Button, ButtonVariant, ScrollArea, Skeleton},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettingsLoad {
    Loading,
    Ready(ViewerUserSettings),
    Error(ClientApiError),
}

#[component]
pub(crate) fn UserSettingsView() -> Element {
    let mut reload = use_signal(|| 0_u64);
    let mut settings = use_signal(|| SettingsLoad::Loading);

    use_effect(move || {
        browser::focus_element("settings-heading".into());
    });
    use_effect(move || {
        let request_generation = reload();
        settings.set(SettingsLoad::Loading);
        spawn(async move {
            let result = ViewerApi::get_settings().await;
            if reload() != request_generation {
                return;
            }
            settings.set(match result {
                Ok(settings) => SettingsLoad::Ready(settings),
                Err(error) => SettingsLoad::Error(error),
            });
        });
    });

    let load = settings();

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
                            p { class: "font-mono font-semibold tracking-widest uppercase",
                                "Effective configuration"
                            }
                        }
                        h1 {
                            id: "settings-heading",
                            class: "mt-1 text-lg font-semibold tracking-tight text-ink focus:outline-none",
                            tabindex: "-1",
                            "User settings"
                        }
                        p { class: "mt-1 max-w-2xl leading-5 text-ink-2",
                            "These values are resolved by git-tools. Edit the configuration file to change them."
                        }
                    }

                    match &load {
                        SettingsLoad::Loading => rsx! {
                            SettingsLoading {}
                        },
                        SettingsLoad::Error(error) => {
                            let message = error.message();
                            rsx! {
                                section { class: "grid min-h-64 place-content-center text-center", role: "alert",
                                    p { class: "font-semibold text-ink", "Settings are unavailable" }
                                    p { class: "mt-1 max-w-md leading-5 text-ink-2", "{message}" }
                                    Button {
                                        class: "mx-auto mt-4",
                                        variant: ButtonVariant::Outline,
                                        onclick: move |_| *reload.write() += 1,
                                        "Try again"
                                    }
                                }
                            }
                        }
                        SettingsLoad::Ready(settings) => rsx! {
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
    let configured_theme = settings.configured_theme.map_or_else(
        || "Not configured".to_owned(),
        |theme| theme_label(theme).to_owned(),
    );
    let configuration_path = settings
        .configuration_path
        .clone()
        .unwrap_or_else(|| "Built-in defaults".to_owned());

    rsx! {
        section {
            class: "overflow-hidden rounded-panel border border-line bg-surface",
            aria_label: "Resolved viewer settings",
            div { class: "flex items-center gap-2 border-b border-line bg-surface-2 px-4 py-3",
                span { class: "text-acc", aria_hidden: "true",
                    FileCog { size: 16 }
                }
                h2 { class: "font-semibold text-ink", "Viewer configuration" }
            }
            dl { class: "divide-y divide-line",
                SettingsRow {
                    term: "Configuration file",
                    value: configuration_path,
                    code: true,
                }
                SettingsRow { term: "Configured theme", value: configured_theme }
                SettingsRow {
                    term: "Effective theme",
                    value: theme_label(settings.effective_theme).to_owned(),
                }
                SettingsRow {
                    term: "Diff layout",
                    value: layout_label(settings.render_options.layout).to_owned(),
                }
                SettingsRow {
                    term: "Diff density",
                    value: density_label(settings.render_options.density).to_owned(),
                }
                SettingsRow {
                    term: "Push confirmation",
                    value: if settings.push_confirmation_required { "Required".to_owned() } else { "Not required".to_owned() },
                }
                div { class: "grid gap-2 px-4 py-4 sm:grid-cols-[14rem_minmax(0,1fr)]",
                    dt { class: "font-semibold text-ink-2", "Default diff exclusions" }
                    dd { class: "m-0 flex min-w-0 flex-wrap gap-1.5",
                        if settings.diff_exclusions.default_extensions.is_empty() {
                            span { class: "text-ink-3", "None" }
                        } else {
                            for extension in &settings.diff_exclusions.default_extensions {
                                code { class: "rounded-sm border border-line-2 bg-sunk px-1.5 py-0.5 font-mono text-xs text-ink",
                                    "*.{extension}"
                                }
                            }
                        }
                    }
                }
            }
        }

        section {
            class: "overflow-hidden rounded-panel border border-line bg-surface",
            aria_label: "Project diff exclusions",
            header { class: "border-b border-line bg-surface-2 px-4 py-3",
                h2 { class: "font-semibold text-ink", "Project exclusions" }
                p { class: "mt-1 text-ink-2",
                    "Repository-specific extension filters, sorted by project name."
                }
            }
            if projects.is_empty() {
                p { class: "px-4 py-5 text-ink-3", "No project-specific exclusions." }
            } else {
                dl { class: "divide-y divide-line",
                    for project in projects {
                        div { class: "grid gap-2 px-4 py-4 sm:grid-cols-[14rem_minmax(0,1fr)]",
                            dt { class: "truncate font-mono font-semibold text-ink",
                                "{project.project_name}"
                            }
                            dd { class: "m-0 flex min-w-0 flex-wrap gap-1.5",
                                if project.extensions.is_empty() {
                                    span { class: "text-ink-3", "None" }
                                } else {
                                    for extension in project.extensions {
                                        code { class: "rounded-sm border border-line-2 bg-sunk px-1.5 py-0.5 font-mono text-xs text-ink",
                                            "*.{extension}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsRow(term: String, value: String, #[props(default)] code: bool) -> Element {
    rsx! {
        div { class: "grid gap-2 px-4 py-4 sm:grid-cols-[14rem_minmax(0,1fr)]",
            dt { class: "font-semibold text-ink-2", "{term}" }
            dd { class: "m-0 min-w-0 break-words text-ink",
                if code {
                    code { class: "font-mono", "{value}" }
                } else {
                    "{value}"
                }
            }
        }
    }
}
