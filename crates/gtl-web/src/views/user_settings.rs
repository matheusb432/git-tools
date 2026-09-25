use dioxus::prelude::*;
use gtl_models::{
    failure::{Failure, SettingsFailure},
    settings::ViewerLanguage,
};
use gtl_wire::viewer::{ViewerRenderOptions, ViewerTheme, ViewerUserSettings};
use lucide_dioxus::{FileCog, Settings};

use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_notice::{client_error_message, client_error_severity, is_invalid_settings},
        field_errors::FieldErrors,
        i18n::{t, use_language},
        ui::{
            Button, ButtonVariant, PageNotice, ScrollArea, SectionedSurface, SectionedSurfaceBody,
            SectionedSurfaceHeader, Skeleton, use_toast,
        },
        viewer_client::ViewerClientError,
        viewer_theme::viewer_theme_label,
    },
    views::viewer_settings_form::{
        SettingsField, ViewerSettingsForm, ViewerSettingsSelection, viewer_settings_patch,
    },
};

#[component]
pub(crate) fn UserSettingsView() -> Element {
    let language = use_language();
    let mut settings = use_resource(viewer_server::get_settings);

    use_effect(move || {
        browser::focus_element(gtl_web_contracts::user_settings::SETTINGS_HEADING_ID.into());
    });
    let pending = settings.state().cloned() == UseResourceState::Pending;
    let load = settings.read();

    rsx! {
        document::Title { {t!(language, "document-title-settings")} }
        main { class: "settings-page-shell h-full",
            ScrollArea { class: "overflow-auto h-full px-4 py-5 sm:px-6",
                div { class: "settings-page-container mx-auto gap-5",
                    header { class: "settings-page-header pb-4",
                        div { class: "flex items-center gap-2 text-acc",
                            span { aria_hidden: "true",
                                Settings { size: 16 }
                            }
                            p { class: "font-semibold tracking-widest uppercase",
                                {t!(language, "settings-eyebrow")}
                            }
                        }
                        h1 {
                            id: gtl_web_contracts::user_settings::SETTINGS_HEADING_ID,
                            class: "settings-page-title mt-1 text-lg font-semibold tracking-tight",
                            tabindex: "-1",
                            {t!(language, "settings-title")}
                        }
                        p { class: "settings-page-description mt-1 leading-5",
                            {t!(language, "settings-description")}
                        }
                    }

                    match (pending, &*load) {
                        (_, None) => rsx! {
                            SettingsLoading {}
                        },
                        (_, Some(Err(error))) if is_invalid_settings(error) => rsx! {
                            crate::views::settings_recovery::SettingsRecovery { onretry: move |()| settings.restart() }
                        },
                        (_, Some(Err(error))) => {
                            let message = client_error_message(error, language);
                            rsx! {
                                PageNotice {
                                    class: "min-h-64",
                                    role: "alert",
                                    title: t!(language, "settings-unavailable"),
                                    message,
                                    Button {
                                        class: "mx-auto mt-4",
                                        variant: ButtonVariant::Outline,
                                        onclick: move |_| settings.restart(),
                                        {t!(language, "action-try-again")}
                                    }
                                }
                            }
                        }
                        (_, Some(Ok(value))) => rsx! {
                            SettingsContent { settings: value.clone(), onchanged: move |()| settings.restart() }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsLoading() -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "grid gap-1",
            role: "status",
            aria_label: t!(language, "settings-loading"),
            for _ in 0..7 {
                div { class: "grid gap-2 border-b border-line py-4 sm:grid-cols-[14rem_minmax(0,1fr)]",
                    Skeleton { class: "h-4 w-3/5" }
                    Skeleton { class: "h-4 w-4/5" }
                }
            }
            span { class: "sr-only", {t!(language, "settings-loading")} }
        }
    }
}

#[component]
fn SettingsContent(settings: ViewerUserSettings, onchanged: EventHandler<()>) -> Element {
    let language = use_language();
    let configuration_path = settings
        .configuration_path
        .clone()
        .unwrap_or_else(|| t!(language, "settings-built-in-defaults"));
    rsx! {
        SettingsEditableForm {
            revision: settings.revision,
            configured_language: settings.language,
            configured_date_format: settings.date_format,
            accessibility: settings.accessibility,
            focus_window_on_diff: settings.focus_window_on_diff,
            push_confirmation_required: settings.push_confirmation_required,
            configured_theme: settings.configured_theme,
            render_options: settings.render_options,
            onchanged,
        }

        SectionedSurface { aria_label: t!(language, "settings-resolved-label"),
            SectionedSurfaceHeader { class: "px-4 py-3",
                SettingsCardHeading {
                    icon: rsx! {
                        FileCog {}
                    },
                    subtitle: t!(language, "settings-resolved-subtitle"),
                    {t!(language, "settings-resolved-title")}
                }
            }
            SectionedSurfaceBody {
                dl { class: "settings-rows",
                    SettingsRow { term: t!(language, "settings-configuration-file"), "{configuration_path}" }
                    SettingsRow { term: t!(language, "settings-effective-theme"),
                        "{viewer_theme_label(settings.effective_theme)}"
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsEditableForm(
    revision: gtl_models::settings::UserSettingsRevision,
    configured_language: ViewerLanguage,
    configured_date_format: gtl_models::settings::ViewerDateFormat,
    accessibility: gtl_models::settings::ViewerAccessibility,
    focus_window_on_diff: bool,
    push_confirmation_required: bool,
    configured_theme: Option<ViewerTheme>,
    render_options: ViewerRenderOptions,
    onchanged: EventHandler<()>,
) -> Element {
    let language = use_language();
    let toast = use_toast();
    let initial = ViewerSettingsSelection::new(
        configured_language,
        configured_date_format,
        configured_theme,
        render_options,
        focus_window_on_diff,
        push_confirmation_required,
        accessibility,
    );
    let mut pending = use_signal(|| false);
    let mut saved = use_signal(|| false);
    let mut failure = use_signal(|| None::<ViewerClientError>);
    let save_error = failure().map(|error| settings_edit_error_message(&error, language));
    let field_errors = failure()
        .as_ref()
        .and_then(rejected_settings)
        .unwrap_or_default();
    let reload_available = failure().is_some_and(|error| settings_edit_reload_available(&error));

    rsx! {
        ViewerSettingsForm {
            initial,
            pending: pending(),
            saved: saved(),
            save_error,
            field_errors,
            reload_available,
            onmodified: move |()| {
                saved.set(false);
                failure.set(None);
            },
            onreload: move |()| {
                saved.set(false);
                failure.set(None);
                onchanged.call(());
            },
            onsubmit: move |selected| {
                if pending() {
                    return;
                }
                saved.set(false);
                failure.set(None);
                let request = viewer_settings_patch(initial, selected, revision);
                pending.set(true);
                spawn(async move {
                    match viewer_server::edit_settings(request).await {
                        Ok(()) => {
                            saved.set(true);
                            toast.ok(t!(language, "settings-saved"));
                            onchanged.call(());
                        }
                        Err(error) => {
                            toast
                                .show(
                                    client_error_severity(&error),
                                    settings_edit_error_message(&error, language),
                                    None,
                                );
                            failure.set(Some(error));
                        }
                    }
                    pending.set(false);
                });
            },
        }
    }
}

/// The form inputs the server rejected, when it named one.
fn rejected_settings(error: &ViewerClientError) -> Option<FieldErrors<SettingsField>> {
    error.failure().and_then(FieldErrors::from_failure)
}

/// Explains a settings edit failure in terms of this page's reload action.
pub(super) fn settings_edit_error_message(
    error: &ViewerClientError,
    language: ViewerLanguage,
) -> String {
    match error {
        ViewerClientError::Failed(Failure::Settings(SettingsFailure::Stale)) => {
            t!(language, "settings-edit-stale")
        }
        ViewerClientError::Failed(Failure::InvalidRequest { .. })
            if rejected_settings(error).is_some() =>
        {
            t!(language, "settings-edit-field-rejected")
        }
        ViewerClientError::Failed(Failure::InvalidRequest { .. }) => {
            t!(language, "settings-edit-rejected")
        }
        ViewerClientError::Failed(Failure::Settings(SettingsFailure::Invalid { .. })) => {
            t!(language, "settings-edit-invalid-file")
        }
        error => client_error_message(error, language),
    }
}

fn settings_edit_reload_available(error: &ViewerClientError) -> bool {
    match error {
        ViewerClientError::Failed(Failure::InvalidRequest { .. }) => {
            rejected_settings(error).is_none()
        }
        ViewerClientError::Failed(Failure::Settings(
            SettingsFailure::Stale | SettingsFailure::Invalid { .. },
        )) => true,
        _ => false,
    }
}

#[component]
fn SettingsCardHeading(
    icon: Option<Element>,
    subtitle: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div {
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
        div { class: "settings-row gap-2 px-4 py-3",
            dt { class: "font-semibold text-ink-2", "{term}" }
            dd { class: "settings-row-value m-0 min-w-0", {children} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejected(field: &str) -> ViewerClientError {
        Failure::InvalidRequest {
            field: field.to_owned(),
        }
        .into()
    }

    #[test]
    fn a_rejected_setting_is_corrected_beside_its_control() {
        let error = rejected("theme");

        assert!(
            rejected_settings(&error)
                .and_then(|errors| errors.message(SettingsField::Theme, ViewerLanguage::EnUs))
                .is_some()
        );
        assert_eq!(
            settings_edit_error_message(&error, ViewerLanguage::EnUs),
            "Correct the highlighted setting and save again."
        );
        assert!(!settings_edit_reload_available(&error));
    }

    #[test]
    fn a_rejection_outside_the_form_offers_a_reload() {
        let error = rejected("expected_revision");

        assert_eq!(rejected_settings(&error), None);
        assert!(settings_edit_reload_available(&error));
    }
}
