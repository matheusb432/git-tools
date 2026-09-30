use dioxus::prelude::*;
use gtl_models::{
    failure::{Failure, SettingsFailure},
    settings::ViewerLanguage,
};
use lucide_dioxus::{ArrowLeft, FileCode};

use crate::{
    app::{
        application_router::{Route, SettingsNavigation},
        user_settings::UserSettings,
    },
    shared::{
        browser,
        failure_notice::{client_error_message, is_invalid_settings},
        field_errors::FieldErrors,
        i18n::{t, use_language},
        ui::{
            Button, ButtonSize, ButtonVariant, PageNotice, PanelDialog, ScrollArea,
            dialog::use_dialog_slot,
        },
        viewer_client::ViewerClientError,
    },
    views::viewer_settings_form::{SettingsField, SettingsSection, ViewerSettingsForm},
};

#[component]
pub(crate) fn UserSettingsView(section: SettingsSection) -> Element {
    let language = use_language();
    let settings = use_context::<UserSettings>();
    let navigator = use_navigator();
    let source = use_dialog_slot::<()>();
    let path = (settings.path)();
    let selection = (settings.selection)();
    let error = (settings.error)();
    rsx! {
        document::Title { {t!(language, "document-title-settings")} }
        main { class: "settings-page-shell h-full",
            header { class: "settings-page-header",
                Button {
                    size: ButtonSize::IconTouch,
                    variant: ButtonVariant::Ghost,
                    aria_label: t!(language, "settings-back"),
                    onclick: move |_| {
                        if navigator.can_go_back() {
                            navigator.go_back();
                        } else {
                            navigator.replace(Route::Projects {});
                        }
                        browser::focus_element("viewer-settings-button".into());
                    },
                    ArrowLeft { size: 20 }
                }
                h1 {
                    id: gtl_web_contracts::user_settings::SETTINGS_HEADING_ID,
                    class: "settings-page-title text-lg font-semibold",
                    tabindex: "-1",
                    onmounted: move |_| browser::focus_element(
                        gtl_web_contracts::user_settings::SETTINGS_HEADING_ID.into(),
                    ),
                    {t!(language, "settings-title")}
                }
                if path.is_some() {
                    button {
                        id: "settings-source-link",
                        r#type: "button",
                        class: "settings-source-link",
                        onclick: move |_| source.open(()),
                        FileCode { size: 14 }
                        "config.toml"
                    }
                }
            }
            if let Some(selected) = selection {
                SettingsEditableForm {
                    selected,
                    section,
                    error: error.clone(),
                    settings,
                }
            } else {
                match error {
                    Some(error) if is_invalid_settings(&error) => rsx! {
                        crate::views::settings_recovery::SettingsRecovery { onretry: settings.reload }
                    },
                    Some(error) => rsx! {
                        PageNotice {
                            class: "min-h-64",
                            role: "alert",
                            title: t!(language, "settings-unavailable"),
                            message: client_error_message(&error, language),
                            Button {
                                variant: ButtonVariant::Outline,
                                onclick: move |_| settings.reload.call(()),
                                {t!(language, "action-try-again")}
                            }
                        }
                    },
                    None => rsx! {},
                }
            }
            if source.subject().is_some() {
                SettingsSource {
                    path: path.unwrap_or_default(),
                    open: source.is_open(),
                    onclose: move |()| source.close(),
                    onclosed: move |()| source.release(),
                }
            }
        }
    }
}

#[component]
fn SettingsSource(
    path: String,
    open: bool,
    onclose: EventHandler<()>,
    onclosed: EventHandler<()>,
) -> Element {
    let language = use_language();
    let source = use_resource(gtl_client::window::read_settings_file);
    rsx! {
        PanelDialog {
            id: "settings-source",
            trigger_id: "settings-source-link",
            open,
            title: t!(language, "settings-configuration-file"),
            onclose,
            onclosed,
            p { class: "break-all px-4 py-3 text-xs text-ink-2", "{path}" }
            ScrollArea { class: "h-[65vh] overflow-auto border-t border-line p-4",
                match &*source.read() {
                    None => rsx! {},
                    Some(Ok(text)) => rsx! {
                        pre { class: "text-xs leading-6 text-ink", "{text}" }
                    },
                    Some(Err(error)) => rsx! {
                        p { role: "alert", {client_error_message(error, language)} }
                    },
                }
            }
        }
    }
}

#[component]
fn SettingsEditableForm(
    selected: crate::views::viewer_settings_form::ViewerSettingsSelection,
    section: SettingsSection,
    error: Option<ViewerClientError>,
    settings: UserSettings,
) -> Element {
    let language = use_language();
    let navigation = use_context::<SettingsNavigation>();
    let error = error.as_ref();
    rsx! {
        ViewerSettingsForm {
            selected,
            section,
            onsection: navigation.select,
            save_error: error.map(|error| settings_edit_error_message(error, language)),
            field_errors: error.and_then(rejected_settings).unwrap_or_default(),
            reload_available: error.is_some_and(settings_edit_reload_available),
            onchange: settings.select,
            onretry: settings.retry,
            onreload: settings.reload,
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
            "Correct the highlighted setting and retry."
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
