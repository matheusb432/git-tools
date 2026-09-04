use dioxus::prelude::*;
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
    pub(crate) theme: Option<ViewerTheme>,
    pub(crate) render_options: ViewerRenderOptions,
}

impl ViewerSettingsSelection {
    pub(crate) const fn new(
        theme: Option<ViewerTheme>,
        render_options: ViewerRenderOptions,
    ) -> Self {
        Self {
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
    onsubmit: EventHandler<ViewerSettingsSelection>,
    onmodified: EventHandler<()>,
) -> Element {
    let mut theme = use_signal(|| initial.theme);
    let mut layout = use_signal(|| initial.render_options.layout);
    let mut density = use_signal(|| initial.render_options.density);

    rsx! {
        form {
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                if pending {
                    return;
                }
                onsubmit
                    .call(ViewerSettingsSelection {
                        theme: theme(),
                        render_options: ViewerRenderOptions {
                            layout: layout(),
                            density: density(),
                        },
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
                div { class: "grid gap-4 p-4 sm:grid-cols-3",
                    div { class: "flex min-w-0 flex-col gap-1.5",
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
                    div { class: "flex min-w-0 flex-col gap-1.5",
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
                    div { class: "flex min-w-0 flex-col gap-1.5",
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
                }
                footer { class: "flex min-h-16 items-center justify-between gap-3 border-t border-line bg-surface-2 px-4 py-3",
                    p {
                        class: "text-xs text-add",
                        role: "status",
                        aria_live: "polite",
                        if saved {
                            span { class: "inline-flex items-center gap-1.5",
                                Check { size: 14 }
                                "Settings saved"
                            }
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

#[cfg(feature = "desktop")]
pub(super) fn viewer_settings_patch(
    current: ViewerSettingsSelection,
    selected: ViewerSettingsSelection,
) -> EditSettingsRequest {
    EditSettingsRequest {
        theme: changed_optional_field(current.theme.as_ref(), selected.theme),
        layout: changed_field(
            &current.render_options.layout,
            selected.render_options.layout,
        ),
        density: changed_field(
            &current.render_options.density,
            selected.render_options.density,
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
    ) -> ViewerSettingsSelection {
        ViewerSettingsSelection::new(theme, ViewerRenderOptions { layout, density })
    }

    #[test]
    fn patch_updates_only_changed_fields() {
        let request = viewer_settings_patch(
            selection(
                Some(gtl_wire::viewer::ViewerTheme::Dark),
                ViewerDiffLayout::Unified,
                ViewerDiffDensity::Compact,
            ),
            selection(None, ViewerDiffLayout::Split, ViewerDiffDensity::Compact),
        );

        assert_eq!(request.theme, FieldUpdate::Clear);
        assert_eq!(request.layout, FieldUpdate::Update(ViewerDiffLayout::Split));
        assert_eq!(request.density, FieldUpdate::Unchanged);
    }
}
