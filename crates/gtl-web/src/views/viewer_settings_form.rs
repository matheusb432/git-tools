use dioxus::prelude::*;
use gtl_models::settings::{
    UserSettingsRevision, ViewerAccessibility, ViewerDateFormat, ViewerLanguage, ViewerScalePercent,
};
use gtl_wire::viewer::{
    EditSettingsRequest, FieldUpdate, ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions,
    ViewerTheme, ViewerUserSettings,
};
use lucide_dioxus::{Columns2, GitBranch, Languages, Paintbrush};

use crate::shared::{
    date_display::date_format_sample,
    field_errors::{FieldErrors, FormField},
    i18n::{language_endonym, t, use_language},
    ui::{Button, ButtonVariant, Checkbox, FieldError, Radio, ScrollArea, Select, SelectOption},
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
    CopyWithLineContext,
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
        Self::CopyWithLineContext,
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
            Self::CopyWithLineContext => "copy_with_line_context",
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
    pub(crate) copy_with_line_context: bool,
    pub(crate) push_confirmation_required: bool,
    pub(crate) theme: Option<ViewerTheme>,
    pub(crate) render_options: ViewerRenderOptions,
}

#[derive(Clone, Copy)]
pub(crate) enum SettingsEdit {
    Language(ViewerLanguage),
    DateFormat(ViewerDateFormat),
    Theme(Option<ViewerTheme>),
    Scale(ViewerScalePercent),
    ReduceMotion(bool),
    Layout(ViewerDiffLayout),
    Density(ViewerDiffDensity),
    WrapLines(bool),
    CopyWithLineContext(bool),
    FocusWindow(bool),
    ConfirmPush(bool),
}

impl SettingsEdit {
    pub(crate) fn apply(self, selected: &mut ViewerSettingsSelection) {
        match self {
            Self::Language(value) => selected.language = value,
            Self::DateFormat(value) => selected.date_format = value,
            Self::Theme(value) => selected.theme = value,
            Self::Scale(value) => selected.accessibility.ui_scale_percent = value,
            Self::ReduceMotion(value) => selected.accessibility.reduce_motion = value,
            Self::Layout(value) => selected.render_options.layout = value,
            Self::Density(value) => selected.render_options.density = value,
            Self::WrapLines(value) => selected.render_options.wrap_lines = value,
            Self::CopyWithLineContext(value) => selected.copy_with_line_context = value,
            Self::FocusWindow(value) => selected.focus_window_on_diff = value,
            Self::ConfirmPush(value) => selected.push_confirmation_required = value,
        }
    }
}

impl From<&ViewerUserSettings> for ViewerSettingsSelection {
    fn from(settings: &ViewerUserSettings) -> Self {
        Self {
            language: settings.language,
            date_format: settings.date_format,
            accessibility: settings.accessibility,
            focus_window_on_diff: settings.focus_window_on_diff,
            copy_with_line_context: settings.copy_with_line_context,
            push_confirmation_required: settings.push_confirmation_required,
            theme: settings.configured_theme,
            render_options: settings.render_options,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    #[default]
    Appearance,
    Locale,
    Snapshots,
    Git,
}

impl SettingsSection {
    fn label(self, language: ViewerLanguage) -> String {
        match self {
            Self::Appearance => t!(language, "settings-appearance"),
            Self::Locale => t!(language, "settings-locale"),
            Self::Snapshots => t!(language, "settings-snapshots"),
            Self::Git => t!(language, "settings-git"),
        }
    }
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Locale => "locale",
            Self::Snapshots => "snapshots",
            Self::Git => "git",
        }
    }
}

impl std::fmt::Display for SettingsSection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

impl std::str::FromStr for SettingsSection {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "appearance" => Ok(Self::Appearance),
            "locale" => Ok(Self::Locale),
            "snapshots" => Ok(Self::Snapshots),
            "git" => Ok(Self::Git),
            _ => Err("unknown settings section"),
        }
    }
}

#[component]
pub(crate) fn ViewerSettingsForm(
    selected: ViewerSettingsSelection,
    section: SettingsSection,
    onsection: EventHandler<SettingsSection>,
    save_error: Option<String>,
    #[props(default)] field_errors: FieldErrors<SettingsField>,
    reload_available: bool,
    onchange: EventHandler<SettingsEdit>,
    onretry: EventHandler<()>,
    onreload: EventHandler<()>,
) -> Element {
    let language = use_language();
    rsx! {
        div { class: "settings-layout",
            nav {
                class: "settings-navigation",
                aria_label: t!(language, "settings-title"),
                for category in [
                    SettingsSection::Appearance,
                    SettingsSection::Locale,
                    SettingsSection::Snapshots,
                    SettingsSection::Git,
                ]
                {
                    button {
                        key: "{category.name()}",
                        r#type: "button",
                        class: "settings-navigation-item",
                        "data-settings-section": category.name(),
                        aria_current: (section == category).then_some("page"),
                        onclick: move |_| onsection.call(category),
                        span { aria_hidden: "true",
                            match category {
                                SettingsSection::Appearance => rsx! {
                                    Paintbrush { size: 17 }
                                },
                                SettingsSection::Locale => rsx! {
                                    Languages { size: 17 }
                                },
                                SettingsSection::Snapshots => rsx! {
                                    Columns2 { size: 17 }
                                },
                                SettingsSection::Git => rsx! {
                                    GitBranch { size: 17 }
                                },
                            }
                        }
                        {category.label(language)}
                    }
                }
            }
            ScrollArea { class: "settings-content overflow-auto",
                div { class: "settings-content-inner",
                    div { class: "settings-section-heading",
                        h2 { class: "text-base font-semibold text-ink", {section.label(language)} }
                        p { class: "settings-save-status", {t!(language, "settings-autosave")} }
                    }
                    if let Some(message) = save_error {
                        div { class: "settings-save-error",
                            p { role: "alert", "{message}" }
                            Button {
                                variant: ButtonVariant::Outline,
                                onclick: move |_| if reload_available { onreload.call(()) } else { onretry.call(()) },
                                if reload_available {
                                    {t!(language, "settings-reload")}
                                } else {
                                    {t!(language, "action-try-again")}
                                }
                            }
                        }
                    }
                    match section {
                        SettingsSection::Appearance => rsx! {
                            SettingsSelectRow { id: "settings-theme", label: t!(language, "settings-theme"),
                                Select {
                                    id: "settings-theme",
                                    aria_label: t!(language, "settings-theme"),
                                    value: selected.theme.map_or("", ViewerTheme::as_str),
                                    options: theme_options(language),
                                    error: field_errors.message(SettingsField::Theme, language),
                                    onchange: move |event: FormEvent| {
                                        let value = event.value();
                                        let theme = if value.is_empty() {
                                            None
                                        } else {
                                            match viewer_theme_from_value(&value) {
                                                Some(theme) => Some(theme),
                                                None => return,
                                            }
                                        };
                                        onchange.call(SettingsEdit::Theme(theme));
                                    },
                                }
                            }
                            SettingsSelectRow { id: "settings-ui-scale", label: t!(language, "settings-ui-scale"),
                                Select {
                                    id: "settings-ui-scale",
                                    aria_label: t!(language, "settings-ui-scale"),
                                    value: selected.accessibility.ui_scale_percent.to_string(),
                                    options: (100..=300)
                                        .step_by(25)
                                        .map(|value| SelectOption::new(value.to_string(), format!("{value}%")))
                                        .collect(),
                                    error: field_errors.message(SettingsField::UiScalePercent, language),
                                    onchange: move |event: FormEvent| {
                                        if let Some(scale) = event
                                            .value()
                                            .parse()
                                            .ok()
                                            .and_then(|value| ViewerScalePercent::try_new(value).ok())
                                        {
                                            onchange.call(SettingsEdit::Scale(scale));
                                        }
                                    },
                                }
                            }
                            div { class: "settings-field-row",
                                Checkbox {
                                    id: "settings-reduce-motion",
                                    label: t!(language, "settings-reduce-motion"),
                                    hint: t!(language, "settings-reduce-motion-hint"),
                                    checked: selected.accessibility.reduce_motion,
                                    error: field_errors.message(SettingsField::ReduceMotion, language),
                                    onchange: move |checked| {
                                        onchange.call(SettingsEdit::ReduceMotion(checked));
                                    },
                                }
                            }
                        },
                        SettingsSection::Locale => rsx! {
                            SettingsRadioGroup {
                                id: "settings-language",
                                label: t!(language, "settings-language"),
                                error: field_errors.message(SettingsField::Language, language),
                                for option in ViewerLanguage::ALL.iter().copied() {
                                    Radio {
                                        key: "{option.as_str()}",
                                        name: "settings-language",
                                        value: option.as_str(),
                                        label: language_endonym(option),
                                        checked: selected.language == option,
                                        onchange: move |()| {
                                            onchange.call(SettingsEdit::Language(option));
                                        },
                                    }
                                }
                            }
                            SettingsRadioGroup {
                                id: "settings-date-format",
                                label: t!(language, "settings-date-format"),
                                hint: t!(language, "settings-date-format-hint"),
                                error: field_errors.message(SettingsField::DateFormat, language),
                                for option in ViewerDateFormat::ALL.iter().copied() {
                                    Radio {
                                        key: "{option.to_string()}",
                                        name: "settings-date-format",
                                        value: option.to_string(),
                                        label: date_format_label(option, language),
                                        checked: selected.date_format == option,
                                        onchange: move |()| {
                                            onchange.call(SettingsEdit::DateFormat(option));
                                        },
                                    }
                                }
                            }
                        },
                        SettingsSection::Snapshots => rsx! {
                            SettingsRadioGroup {
                                id: "settings-layout",
                                label: t!(language, "settings-layout"),
                                error: field_errors.message(SettingsField::Layout, language),
                                for layout in [ViewerDiffLayout::Unified, ViewerDiffLayout::Split] {
                                    Radio {
                                        key: "{layout.as_str()}",
                                        name: "settings-layout",
                                        value: layout.as_str(),
                                        label: match layout {
                                            ViewerDiffLayout::Unified => t!(language, "settings-layout-unified"),
                                            ViewerDiffLayout::Split => t!(language, "settings-layout-split"),
                                        },
                                        checked: selected.render_options.layout == layout,
                                        onchange: move |()| {
                                            onchange.call(SettingsEdit::Layout(layout));
                                        },
                                    }
                                }
                            }
                            SettingsRadioGroup {
                                id: "settings-density",
                                label: t!(language, "settings-density"),
                                error: field_errors.message(SettingsField::Density, language),
                                for density in [ViewerDiffDensity::Compact, ViewerDiffDensity::Full] {
                                    Radio {
                                        key: "{density.as_str()}",
                                        name: "settings-density",
                                        value: density.as_str(),
                                        label: match density {
                                            ViewerDiffDensity::Compact => t!(language, "settings-density-compact"),
                                            ViewerDiffDensity::Full => t!(language, "settings-density-full"),
                                        },
                                        checked: selected.render_options.density == density,
                                        onchange: move |()| {
                                            onchange.call(SettingsEdit::Density(density));
                                        },
                                    }
                                }
                            }
                            div { class: "settings-field-row",
                                Checkbox {
                                    id: "settings-wrap-lines",
                                    label: t!(language, "settings-wrap-lines"),
                                    hint: t!(language, "settings-wrap-lines-hint"),
                                    checked: selected.render_options.wrap_lines,
                                    error: field_errors.message(SettingsField::WrapLines, language),
                                    onchange: move |checked| {
                                        onchange.call(SettingsEdit::WrapLines(checked));
                                    },
                                }
                            }
                            div { class: "settings-field-row",
                                Checkbox {
                                    id: "settings-copy-with-line-context",
                                    label: t!(language, "settings-copy-with-line-context"),
                                    hint: t!(language, "settings-copy-with-line-context-hint"),
                                    checked: selected.copy_with_line_context,
                                    error: field_errors.message(SettingsField::CopyWithLineContext, language),
                                    onchange: move |checked| {
                                        onchange.call(SettingsEdit::CopyWithLineContext(checked));
                                    },
                                }
                            }
                        },
                        SettingsSection::Git => rsx! {
                            div { class: "settings-field-row",
                                Checkbox {
                                    id: "settings-focus-window-on-diff",
                                    label: t!(language, "settings-focus-window"),
                                    hint: t!(language, "settings-focus-window-hint"),
                                    checked: selected.focus_window_on_diff,
                                    error: field_errors.message(SettingsField::FocusWindowOnDiff, language),
                                    onchange: move |checked| {
                                        onchange.call(SettingsEdit::FocusWindow(checked));
                                    },
                                }
                            }
                            div { class: "settings-field-row",
                                Checkbox {
                                    id: "settings-push-confirmation",
                                    label: t!(language, "settings-push-confirmation"),
                                    hint: t!(language, "settings-push-confirmation-hint"),
                                    checked: !selected.push_confirmation_required,
                                    error: field_errors.message(SettingsField::PushConfirmationRequired, language),
                                    onchange: move |checked: bool| {
                                        onchange.call(SettingsEdit::ConfirmPush(!checked));
                                    },
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsSelectRow(id: String, label: String, children: Element) -> Element {
    rsx! {
        div { class: "settings-field-row settings-select-row",
            label { r#for: id, class: "text-ink", "{label}" }
            div { class: "settings-select-control", {children} }
        }
    }
}

#[component]
fn SettingsRadioGroup(
    id: String,
    label: String,
    hint: Option<String>,
    error: Option<String>,
    children: Element,
) -> Element {
    let hint_id = format!("{id}-hint");
    let error_id = format!("{id}-error");
    let described_by = if hint.is_some() {
        format!("{hint_id} {error_id}")
    } else {
        error_id.clone()
    };
    let invalid = error.is_some();
    rsx! {
        fieldset {
            id,
            class: "settings-field-row settings-radio-group",
            aria_describedby: described_by,
            aria_invalid: invalid.then_some("true"),
            legend { class: "text-ink", "{label}" }
            if let Some(hint) = hint {
                p { id: hint_id, class: "text-xs leading-5 text-ink-2", "{hint}" }
            }
            div { class: "settings-radio-options", {children} }
            FieldError { id: error_id, message: error }
        }
    }
}

pub(crate) fn viewer_settings_patch(
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
        copy_with_line_context: changed_field(
            &current.copy_with_line_context,
            selected.copy_with_line_context,
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

fn date_format_label(format: ViewerDateFormat, language: ViewerLanguage) -> String {
    let sample = date_format_sample(format, language);
    match format {
        ViewerDateFormat::Iso => t!(language, "settings-date-format-iso", sample = sample),
        ViewerDateFormat::DayFirst => {
            t!(language, "settings-date-format-day-first", sample = sample)
        }
        ViewerDateFormat::MonthFirst => t!(
            language,
            "settings-date-format-month-first",
            sample = sample
        ),
        ViewerDateFormat::Relative => {
            t!(language, "settings-date-format-relative", sample = sample)
        }
    }
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
        ViewerSettingsSelection {
            language: gtl_models::settings::ViewerLanguage::EnUs,
            date_format: gtl_models::settings::ViewerDateFormat::Iso,
            theme,
            render_options: ViewerRenderOptions {
                wrap_lines: false,
                layout,
                density,
            },
            focus_window_on_diff: true,
            copy_with_line_context: true,
            push_confirmation_required,
            accessibility: gtl_models::settings::ViewerAccessibility::default(),
        }
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

    #[test]
    fn disabling_copy_context_updates_only_that_setting() {
        let current = selection(
            None,
            ViewerDiffLayout::Unified,
            ViewerDiffDensity::Compact,
            true,
        );
        let selected = ViewerSettingsSelection {
            copy_with_line_context: false,
            ..current
        };
        let request = viewer_settings_patch(
            current,
            selected,
            gtl_models::settings::UserSettingsRevision::from_digest([0x45; 32]),
        );
        assert_eq!(request.copy_with_line_context, FieldUpdate::Update(false));
        assert_eq!(request.wrap_lines, FieldUpdate::Unchanged);
        assert_eq!(request.layout, FieldUpdate::Unchanged);
    }
}
