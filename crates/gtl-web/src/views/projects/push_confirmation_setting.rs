use dioxus::prelude::*;
use gtl_models::paths::ProjectName;
use gtl_wire::viewer::{EditSettingsRequest, FieldUpdate};

use crate::{
    entities::diffs::viewer_server,
    shared::{
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonVariant, Checkbox},
        viewer_client::captured_client_error,
    },
};

#[component]
pub(super) fn ProjectPushConfirmationSetting(project: ProjectName, id: String) -> Element {
    let language = use_language();
    let mut selected = use_signal(|| None::<bool>);
    let mut settings = use_resource(viewer_server::get_settings);
    let mut save = use_action(move |request: EditSettingsRequest| async move {
        let result = viewer_server::edit_settings(request).await;
        settings.restart();
        result
    });
    let loaded = settings
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let saved = loaded.as_ref().is_some_and(|settings| {
        settings
            .viewer_push_no_confirmation_projects
            .contains(&project)
    });
    use_effect(use_reactive((&saved,), move |(saved,)| {
        if selected().is_some_and(|choice| choice == saved) {
            selected.set(None);
        }
    }));
    let checked = selected().unwrap_or(saved);
    let error = save
        .value()
        .and_then(Result::err)
        .as_ref()
        .and_then(captured_client_error)
        .map(|error| client_error_message(error, language))
        .or_else(|| {
            settings
                .read()
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .map(|error| client_error_message(error, language))
        });
    let pending = save.pending();
    rsx! {
        div { class: "project-push-setting",
            Checkbox {
                id,
                label: t!(language, "projects-viewer-push-no-confirmation"),
                hint: t!(language, "projects-viewer-push-no-confirmation-hint"),
                checked,
                disabled: pending || loaded.is_none(),
                error: error.clone(),
                onchange: move |no_confirmation| {
                    if save.pending() {
                        return;
                    }
                    let Some(settings) = loaded.as_ref() else {
                        return;
                    };
                    let mut projects = settings.viewer_push_no_confirmation_projects.clone();
                    projects.retain(|name| name != &project);
                    if no_confirmation {
                        projects.push(project.clone());
                    }
                    selected.set(Some(no_confirmation));
                    save.call(EditSettingsRequest {
                        expected_revision: Some(settings.revision),
                        viewer_push_no_confirmation_projects: FieldUpdate::Update(projects),
                        ..EditSettingsRequest::default()
                    });
                },
            }
            if error.is_some() {
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Bare,
                    onclick: move |_| {
                        selected.set(None);
                        save.reset();
                        settings.restart();
                    },
                    {t!(language, "settings-reload")}
                }
            }
        }
    }
}
