use dioxus::prelude::*;
#[cfg(feature = "desktop")]
use gtl_models::settings::UserSettingsRevision;
#[cfg(feature = "desktop")]
use gtl_wire::viewer::{EditSettingsRequest, FieldUpdate};
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions, ViewerTheme};
use lucide_dioxus::Check;

use crate::shared::{
    ui::{Button, ButtonState, ButtonType, FieldLabel, Select, SelectOption},
    viewer_theme::{VIEWER_THEME_OPTIONS, viewer_theme_from_value, viewer_theme_label},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewerSettingsSelection {
    pub(crate) focus_window_on_diff: bool,
    pub(crate) push_confirmation_required: bool,
    pub(crate) theme: Option<ViewerTheme>,
    pub(crate) render_options: ViewerRenderOptions,
}

impl ViewerSettingsSelection {
    pub(crate) const fn new(
        theme: Option<ViewerTheme>,
        render_options: ViewerRenderOptions,
        focus_window_on_diff: bool,
        push_confirmation_required: bool,
    ) -> Self {
        Self {
            focus_window_on_diff,
            push_confirmation_required,
            theme,
            render_options,
        }
    }
}

#[component]
pub(crate) fn ViewerSettingsForm(
    initial: ViewerSettingsSelection,
    pending: bool,
    saved: bool,
    save_error: Option<String>,
    reload_available: bool,
    onsubmit: EventHandler<ViewerSettingsSelection>,
    onmodified: EventHandler<()>,
    onreload: EventHandler<()>,
) -> Element {
    let mut focus_window_on_diff = use_signal(|| initial.focus_window_on_diff);
    let mut push_confirmation_required = use_signal(|| initial.push_confirmation_required);
    let mut theme = use_signal(|| initial.theme);
    let mut layout = use_signal(|| initial.render_options.layout);
    let mut density = use_signal(|| initial.render_options.density);
    let mut wrap_lines = use_signal(|| initial.render_options.wrap_lines);

    rsx! {
        form {
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                if pending {
                    return;
                }
                onsubmit
                    .call(ViewerSettingsSelection {
                        focus_window_on_diff: focus_window_on_diff(),
                        push_confirmation_required: push_confirmation_required(),
                        theme: theme(),
                        render_options: ViewerRenderOptions {
                            wrap_lines: wrap_lines(),
                            layout: layout(),
                            density: density(),
                        },
                    });
            },
            section {
                class: "settings-card",
                aria_label: "Editable application settings",
                header { class: "settings-card-header px-4 py-3",
                    h2 { class: "font-semibold text-ink", "Viewer and command preferences" }
                    p { class: "mt-0.5 text-xs leading-5 text-ink-3",
                        "Choose how diffs appear, when the desktop window comes forward, and whether pushes need confirmation."
                    }
                }
                div { class: "settings-form-grid gap-4 p-4",
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-theme",
                            label: "Theme",
                            hint: "Choose the palette used for diff chrome and code.",
                        }
                        Select {
                            id: "settings-theme",
                            name: "theme",
                            aria_label: "Theme",
                            value: theme().map_or("", ViewerTheme::as_str),
                            options: theme_options(),
                            error: None,
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                let value = event.value();
                                let selected = if value.is_empty() {
                                    None
                                } else {
                                    let Some(theme) = viewer_theme_from_value(&value) else {
                                        return;
                                    };
                                    Some(theme)
                                };
                                theme.set(selected);
                                onmodified.call(());
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-layout",
                            label: "Layout",
                            hint: "Choose how old and new lines share the canvas.",
                        }
                        Select {
                            id: "settings-layout",
                            name: "layout",
                            aria_label: "Layout",
                            value: layout().as_str(),
                            options: layout_options(),
                            error: None,
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                let Some(selected) = parse_layout(&event.value()) else {
                                    return;
                                };
                                layout.set(selected);
                                onmodified.call(());
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-wrap-lines",
                            label: "Wrap lines",
                            hint: "Fit source lines to the available width or scroll horizontally.",
                        }
                        Select {
                            id: "settings-wrap-lines",
                            name: "wrap_lines",
                            "data-testid": gtl_web_contracts::test_ids::VIEWER_SETTINGS_WRAP_LINES.value(),
                            aria_label: "Wrap lines",
                            value: if wrap_lines() { "true" } else { "false" },
                            options: vec![SelectOption::new("false", "Off"), SelectOption::new("true", "On")],
                            error: None,
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                if let Ok(selected) = event.value().parse::<bool>() {
                                    wrap_lines.set(selected);
                                    onmodified.call(());
                                }
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-density",
                            label: "View",
                            hint: "Show changed regions or the complete file.",
                        }
                        Select {
                            id: "settings-density",
                            name: "density",
                            aria_label: "View",
                            value: density().as_str(),
                            options: density_options(),
                            error: None,
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                let Some(selected) = parse_density(&event.value()) else {
                                    return;
                                };
                                density.set(selected);
                                onmodified.call(());
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-focus-window-on-diff",
                            label: "Focus window when opening a diff",
                            hint: "Bring the desktop window forward when a command opens a diff.",
                        }
                        Select {
                            id: "settings-focus-window-on-diff",
                            name: "focus_window_on_diff",
                            aria_label: "Focus window when opening a diff",
                            value: if focus_window_on_diff() { "true" } else { "false" },
                            options: vec![SelectOption::new("false", "Off"), SelectOption::new("true", "On")],
                            error: None,
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                if let Ok(selected) = event.value().parse::<bool>() {
                                    focus_window_on_diff.set(selected);
                                    onmodified.call(());
                                }
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-push-confirmation",
                            label: "Confirm before push",
                            hint: "Require confirmation before pushing the current repository.",
                        }
                        Select {
                            id: "settings-push-confirmation",
                            name: "push_confirmation_required",
                            aria_label: "Confirm before push",
                            value: if push_confirmation_required() { "true" } else { "false" },
                            options: vec![
                                SelectOption::new("true", "Required"),
                                SelectOption::new("false", "Not required"),
                            ],
                            error: None,
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                if let Ok(selected) = event.value().parse::<bool>() {
                                    push_confirmation_required.set(selected);
                                    onmodified.call(());
                                }
                            },
                        }
                    }
                }
                footer { class: "settings-form-footer min-h-16 flex-wrap gap-3 px-4 py-3",
                    div { class: "min-w-0 flex-1",
                        if let Some(message) = save_error {
                            p { class: "text-sm text-del", role: "alert", "{message}" }
                        } else if saved {
                            p {
                                class: "text-xs text-add",
                                role: "status",
                                aria_live: "polite",
                                span { class: "inline-flex items-center gap-1.5",
                                    Check { size: 14 }
                                    "Settings saved"
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        if reload_available {
                            Button {
                                variant: crate::shared::ui::ButtonVariant::Outline,
                                disabled: pending,
                                onclick: move |_| onreload.call(()),
                                "Reload settings"
                            }
                        }
                        Button {
                            button_type: ButtonType::Submit,
                            state: if pending { ButtonState::Loading } else { ButtonState::Enabled },
                            "Save settings"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
pub(super) fn viewer_settings_patch(
    current: ViewerSettingsSelection,
    selected: ViewerSettingsSelection,
    expected_revision: UserSettingsRevision,
) -> EditSettingsRequest {
    EditSettingsRequest {
        expected_revision: Some(expected_revision),
        focus_window_on_diff: changed_field(
            &current.focus_window_on_diff,
            selected.focus_window_on_diff,
        ),
        wrap_lines: changed_field(
            &current.render_options.wrap_lines,
            selected.render_options.wrap_lines,
        ),
        theme: changed_optional_field(current.theme.as_ref(), selected.theme),
        layout: changed_field(
            &current.render_options.layout,
            selected.render_options.layout,
        ),
        density: changed_field(
            &current.render_options.density,
            selected.render_options.density,
        ),
        push_confirmation_required: changed_field(
            &current.push_confirmation_required,
            selected.push_confirmation_required,
        ),
        ..EditSettingsRequest::default()
    }
}

#[cfg(feature = "desktop")]
fn changed_field<T: PartialEq>(current: &T, selected: T) -> FieldUpdate<T> {
    if current == &selected {
        FieldUpdate::Unchanged
    } else {
        FieldUpdate::Update(selected)
    }
}

#[cfg(feature = "desktop")]
fn changed_optional_field<T: PartialEq>(
    current: Option<&T>,
    selected: Option<T>,
) -> FieldUpdate<T> {
    if current == selected.as_ref() {
        FieldUpdate::Unchanged
    } else {
        selected.map_or(FieldUpdate::Clear, FieldUpdate::Update)
    }
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

fn theme_options() -> Vec<SelectOption> {
    std::iter::once(SelectOption::new("", "Built-in default (Dark)"))
        .chain(
            VIEWER_THEME_OPTIONS
                .into_iter()
                .map(|theme| SelectOption::new(theme.as_str(), viewer_theme_label(theme))),
        )
        .collect()
}

fn layout_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("unified", "Unified"),
        SelectOption::new("split", "Side by side"),
    ]
}

fn density_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("compact", "Changes only"),
        SelectOption::new("full", "Full file"),
    ]
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use gtl_wire::viewer::{FieldUpdate, ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions};

    use super::{ViewerSettingsSelection, viewer_settings_patch};

    fn selection(
        theme: Option<gtl_wire::viewer::ViewerTheme>,
        layout: ViewerDiffLayout,
        density: ViewerDiffDensity,
        push_confirmation_required: bool,
    ) -> ViewerSettingsSelection {
        ViewerSettingsSelection::new(
            theme,
            ViewerRenderOptions {
                wrap_lines: false,
                layout,
                density,
            },
            true,
            push_confirmation_required,
        )
    }

    #[test]
    fn patch_updates_only_changed_fields() {
        let request = viewer_settings_patch(
            selection(
                Some(gtl_wire::viewer::ViewerTheme::Dark),
                ViewerDiffLayout::Unified,
                ViewerDiffDensity::Compact,
                true,
            ),
            selection(
                None,
                ViewerDiffLayout::Split,
                ViewerDiffDensity::Compact,
                false,
            ),
            gtl_models::settings::UserSettingsRevision::from_digest([0x44; 32]),
        );

        assert_eq!(
            request.expected_revision,
            Some(gtl_models::settings::UserSettingsRevision::from_digest(
                [0x44; 32]
            ))
        );
        assert_eq!(request.theme, FieldUpdate::Clear);
        assert_eq!(request.layout, FieldUpdate::Update(ViewerDiffLayout::Split));
        assert_eq!(request.density, FieldUpdate::Unchanged);
        assert_eq!(
            request.push_confirmation_required,
            FieldUpdate::Update(false)
        );
    }
}
