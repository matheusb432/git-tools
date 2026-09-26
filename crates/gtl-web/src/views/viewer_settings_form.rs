use dioxus::prelude::*;
use gtl_models::settings::{
    UserSettingsRevision, ViewerAccessibility, ViewerDateFormat, ViewerLanguage, ViewerScalePercent,
};
use gtl_wire::viewer::{
    EditSettingsRequest, FieldUpdate, ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions,
    ViewerTheme,
};
use lucide_dioxus::Check;

use crate::shared::{
    date_display::date_format_sample,
    field_errors::{FieldErrors, FormField},
    i18n::{language_endonym, t, use_language},
    ui::{
        Button, ButtonState, ButtonType, FieldLabel, SectionedSurface, SectionedSurfaceBody,
        SectionedSurfaceFooter, SectionedSurfaceHeader, Select, SelectOption,
    },
    viewer_theme::{VIEWER_THEME_OPTIONS, viewer_theme_from_value, viewer_theme_label},
};

/// An input of the viewer settings form, named after its `EditSettingsRequest` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsField {
    Language,
    DateFormat,
    UiScalePercent,
    ReduceMotion,
    Theme,
    Layout,
    WrapLines,
    Density,
    FocusWindowOnDiff,
    PushConfirmationRequired,
}

impl FormField for SettingsField {
    const ALL: &'static [Self] = &[
        Self::Language,
        Self::DateFormat,
        Self::UiScalePercent,
        Self::ReduceMotion,
        Self::Theme,
        Self::Layout,
        Self::WrapLines,
        Self::Density,
        Self::FocusWindowOnDiff,
        Self::PushConfirmationRequired,
    ];

    fn request_field(self) -> &'static str {
        match self {
            Self::Language => "language",
            Self::DateFormat => "date_format",
            Self::UiScalePercent => "ui_scale_percent",
            Self::ReduceMotion => "reduce_motion",
            Self::Theme => "theme",
            Self::Layout => "layout",
            Self::WrapLines => "wrap_lines",
            Self::Density => "density",
            Self::FocusWindowOnDiff => "focus_window_on_diff",
            Self::PushConfirmationRequired => "push_confirmation_required",
        }
    }

    fn correction(self, language: ViewerLanguage) -> String {
        t!(language, "settings-field-correction")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewerSettingsSelection {
    pub(crate) language: ViewerLanguage,
    pub(crate) date_format: ViewerDateFormat,
    pub(crate) accessibility: ViewerAccessibility,
    pub(crate) focus_window_on_diff: bool,
    pub(crate) push_confirmation_required: bool,
    pub(crate) theme: Option<ViewerTheme>,
    pub(crate) render_options: ViewerRenderOptions,
}

impl ViewerSettingsSelection {
    pub(crate) const fn new(
        language: ViewerLanguage,
        date_format: ViewerDateFormat,
        theme: Option<ViewerTheme>,
        render_options: ViewerRenderOptions,
        focus_window_on_diff: bool,
        push_confirmation_required: bool,
        accessibility: ViewerAccessibility,
    ) -> Self {
        Self {
            language,
            date_format,
            accessibility,
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
    #[props(default)] field_errors: FieldErrors<SettingsField>,
    reload_available: bool,
    onsubmit: EventHandler<ViewerSettingsSelection>,
    onmodified: EventHandler<()>,
    onreload: EventHandler<()>,
) -> Element {
    let language = use_language();
    let mut language_setting = use_signal(|| initial.language);
    let mut date_format = use_signal(|| initial.date_format);
    let mut ui_scale_percent = use_signal(|| initial.accessibility.ui_scale_percent);
    let mut reduce_motion = use_signal(|| initial.accessibility.reduce_motion);
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
                        language: language_setting(),
                        date_format: date_format(),
                        accessibility: ViewerAccessibility {
                            ui_scale_percent: ui_scale_percent(),
                            reduce_motion: reduce_motion(),
                        },
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
            SectionedSurface { aria_label: t!(language, "settings-form-label"),
                SectionedSurfaceHeader { class: "px-4 py-3",
                    h2 { class: "font-semibold text-ink", {t!(language, "settings-form-title")} }
                    p { class: "mt-0.5 text-xs leading-5 text-ink-3",
                        {t!(language, "settings-form-description")}
                    }
                }
                SectionedSurfaceBody { class: "settings-form-grid gap-4 p-4",
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-language",
                            label: t!(language, "settings-language"),
                            hint: t!(language, "settings-language-hint"),
                        }
                        Select {
                            id: "settings-language",
                            name: SettingsField::Language.request_field(),
                            aria_label: t!(language, "settings-language"),
                            value: language_setting().as_str(),
                            options: language_options(),
                            error: field_errors.message(SettingsField::Language, language),
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                if let Ok(selected) = event.value().parse() {
                                    language_setting.set(selected);
                                    onmodified.call(());
                                }
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-date-format",
                            label: t!(language, "settings-date-format"),
                            hint: t!(language, "settings-date-format-hint"),
                        }
                        Select {
                            id: "settings-date-format",
                            name: SettingsField::DateFormat.request_field(),
                            aria_label: t!(language, "settings-date-format"),
                            value: date_format().to_string(),
                            options: date_format_options(language),
                            error: field_errors.message(SettingsField::DateFormat, language),
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                if let Ok(selected) = event.value().parse() {
                                    date_format.set(selected);
                                    onmodified.call(());
                                }
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-ui-scale",
                            label: t!(language, "settings-ui-scale"),
                            hint: t!(language, "settings-ui-scale-hint"),
                        }
                        Select {
                            id: "settings-ui-scale",
                            name: SettingsField::UiScalePercent.request_field(),
                            aria_label: t!(language, "settings-ui-scale"),
                            value: ui_scale_percent().to_string(),
                            options: (100..=300)
                                .step_by(25)
                                .map(|value| SelectOption::new(value.to_string(), format!("{value}%")))
                                .collect(),
                            error: field_errors.message(SettingsField::UiScalePercent, language),
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                if let Some(selected) = event
                                    .value()
                                    .parse()
                                    .ok()
                                    .and_then(|value| ViewerScalePercent::try_new(value).ok())
                                {
                                    ui_scale_percent.set(selected);
                                    onmodified.call(());
                                }
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-reduce-motion",
                            label: t!(language, "settings-reduce-motion"),
                            hint: t!(language, "settings-reduce-motion-hint"),
                        }
                        Select {
                            id: "settings-reduce-motion",
                            name: SettingsField::ReduceMotion.request_field(),
                            aria_label: t!(language, "settings-reduce-motion"),
                            value: if reduce_motion() { "true" } else { "false" },
                            options: vec![
                                SelectOption::new("false", t!(language, "settings-reduce-motion-system")),
                                SelectOption::new("true", t!(language, "settings-reduce-motion-always")),
                            ],
                            error: field_errors.message(SettingsField::ReduceMotion, language),
                            disabled: pending,
                            onchange: move |event: FormEvent| {
                                if let Ok(selected) = event.value().parse::<bool>() {
                                    reduce_motion.set(selected);
                                    onmodified.call(());
                                }
                            },
                        }
                    }
                    div { class: "settings-form-field min-w-0 gap-1.5",
                        FieldLabel {
                            for_id: "settings-theme",
                            label: t!(language, "settings-theme"),
                            hint: t!(language, "settings-theme-hint"),
                        }
                        Select {
                            id: "settings-theme",
                            name: SettingsField::Theme.request_field(),
                            aria_label: t!(language, "settings-theme"),
                            value: theme().map_or("", ViewerTheme::as_str),
                            options: theme_options(language),
                            error: field_errors.message(SettingsField::Theme, language),
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
                            label: t!(language, "settings-layout"),
                            hint: t!(language, "settings-layout-hint"),
                        }
                        Select {
                            id: "settings-layout",
                            name: SettingsField::Layout.request_field(),
                            aria_label: t!(language, "settings-layout"),
                            value: layout().as_str(),
                            options: layout_options(language),
                            error: field_errors.message(SettingsField::Layout, language),
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
                            label: t!(language, "settings-wrap-lines"),
                            hint: t!(language, "settings-wrap-lines-hint"),
                        }
                        Select {
                            id: "settings-wrap-lines",
                            name: SettingsField::WrapLines.request_field(),
                            aria_label: t!(language, "settings-wrap-lines"),
                            value: if wrap_lines() { "true" } else { "false" },
                            options: on_off_options(language),
                            error: field_errors.message(SettingsField::WrapLines, language),
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
                            label: t!(language, "settings-density"),
                            hint: t!(language, "settings-density-hint"),
                        }
                        Select {
                            id: "settings-density",
                            name: SettingsField::Density.request_field(),
                            aria_label: t!(language, "settings-density"),
                            value: density().as_str(),
                            options: density_options(language),
                            error: field_errors.message(SettingsField::Density, language),
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
                            label: t!(language, "settings-focus-window"),
                            hint: t!(language, "settings-focus-window-hint"),
                        }
                        Select {
                            id: "settings-focus-window-on-diff",
                            name: SettingsField::FocusWindowOnDiff.request_field(),
                            aria_label: t!(language, "settings-focus-window"),
                            value: if focus_window_on_diff() { "true" } else { "false" },
                            options: on_off_options(language),
                            error: field_errors.message(SettingsField::FocusWindowOnDiff, language),
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
                            label: t!(language, "settings-push-confirmation"),
                            hint: t!(language, "settings-push-confirmation-hint"),
                        }
                        Select {
                            id: "settings-push-confirmation",
                            name: SettingsField::PushConfirmationRequired.request_field(),
                            aria_label: t!(language, "settings-push-confirmation"),
                            value: if push_confirmation_required() { "true" } else { "false" },
                            options: vec![
                                SelectOption::new("true", t!(language, "settings-push-confirmation-required")),
                                SelectOption::new(
                                    "false",
                                    t!(language, "settings-push-confirmation-not-required"),
                                ),
                            ],
                            error: field_errors.message(SettingsField::PushConfirmationRequired, language),
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
                SectionedSurfaceFooter { class: "settings-form-footer min-h-16 flex-wrap gap-3 px-4 py-3",
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
                                    {t!(language, "settings-saved")}
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
                                {t!(language, "settings-reload")}
                            }
                        }
                        Button {
                            button_type: ButtonType::Submit,
                            state: if pending { ButtonState::Loading } else { ButtonState::Enabled },
                            {t!(language, "settings-save")}
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn viewer_settings_patch(
    current: ViewerSettingsSelection,
    selected: ViewerSettingsSelection,
    expected_revision: UserSettingsRevision,
) -> EditSettingsRequest {
    EditSettingsRequest {
        expected_revision: Some(expected_revision),
        language: changed_field(&current.language, selected.language),
        date_format: changed_field(&current.date_format, selected.date_format),
        ui_scale_percent: changed_field(
            &current.accessibility.ui_scale_percent,
            selected.accessibility.ui_scale_percent,
        ),
        reduce_motion: changed_field(
            &current.accessibility.reduce_motion,
            selected.accessibility.reduce_motion,
        ),
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

fn changed_field<T: PartialEq>(current: &T, selected: T) -> FieldUpdate<T> {
    if current == &selected {
        FieldUpdate::Unchanged
    } else {
        FieldUpdate::Update(selected)
    }
}

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

fn language_options() -> Vec<SelectOption> {
    ViewerLanguage::ALL
        .iter()
        .map(|language| SelectOption::new(language.as_str(), language_endonym(*language)))
        .collect()
}

fn date_format_options(language: ViewerLanguage) -> Vec<SelectOption> {
    ViewerDateFormat::ALL
        .iter()
        .map(|format| {
            let sample = date_format_sample(*format, language);
            let label = match format {
                ViewerDateFormat::Iso => t!(language, "settings-date-format-iso", sample = sample),
                ViewerDateFormat::DayFirst => {
                    t!(language, "settings-date-format-day-first", sample = sample)
                }
                ViewerDateFormat::MonthFirst => {
                    t!(
                        language,
                        "settings-date-format-month-first",
                        sample = sample
                    )
                }
                ViewerDateFormat::Relative => {
                    t!(language, "settings-date-format-relative", sample = sample)
                }
            };
            SelectOption::new(format.to_string(), label)
        })
        .collect()
}

fn on_off_options(language: ViewerLanguage) -> Vec<SelectOption> {
    vec![
        SelectOption::new("false", t!(language, "settings-off")),
        SelectOption::new("true", t!(language, "settings-on")),
    ]
}

fn theme_options(language: ViewerLanguage) -> Vec<SelectOption> {
    std::iter::once(SelectOption::new(
        "",
        t!(language, "settings-theme-default"),
    ))
    .chain(
        VIEWER_THEME_OPTIONS
            .into_iter()
            .map(|theme| SelectOption::new(theme.as_str(), viewer_theme_label(theme))),
    )
    .collect()
}

fn layout_options(language: ViewerLanguage) -> Vec<SelectOption> {
    vec![
        SelectOption::new("unified", t!(language, "settings-layout-unified")),
        SelectOption::new("split", t!(language, "settings-layout-split")),
    ]
}

fn density_options(language: ViewerLanguage) -> Vec<SelectOption> {
    vec![
        SelectOption::new("compact", t!(language, "settings-density-compact")),
        SelectOption::new("full", t!(language, "settings-density-full")),
    ]
}

#[cfg(test)]
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
            gtl_models::settings::ViewerLanguage::EnUs,
            gtl_models::settings::ViewerDateFormat::Iso,
            theme,
            ViewerRenderOptions {
                wrap_lines: false,
                layout,
                density,
            },
            true,
            push_confirmation_required,
            gtl_models::settings::ViewerAccessibility::default(),
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
            ViewerSettingsSelection {
                language: gtl_models::settings::ViewerLanguage::PtBr,
                date_format: gtl_models::settings::ViewerDateFormat::Relative,
                ..selection(
                    None,
                    ViewerDiffLayout::Split,
                    ViewerDiffDensity::Compact,
                    false,
                )
            },
            gtl_models::settings::UserSettingsRevision::from_digest([0x44; 32]),
        );

        assert_eq!(
            request.expected_revision,
            Some(gtl_models::settings::UserSettingsRevision::from_digest(
                [0x44; 32]
            ))
        );
        assert_eq!(
            request.language,
            FieldUpdate::Update(gtl_models::settings::ViewerLanguage::PtBr)
        );
        assert_eq!(
            request.date_format,
            FieldUpdate::Update(gtl_models::settings::ViewerDateFormat::Relative)
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
