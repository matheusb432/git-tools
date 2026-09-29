use dioxus::prelude::*;
use gtl_models::settings::{ProjectsPageSize, ProjectsPreferences, ProjectsSort};
use gtl_wire::viewer::{EditSettingsRequest, FieldUpdate, ViewerUserSettings};

use crate::{
    app::application_layout::ViewerContext, entities::diffs::viewer_server,
    shared::viewer_client::ViewerClientError,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ProjectsPresentation {
    pub(super) page_size: Memo<ProjectsPageSize>,
    pub(super) sort: Memo<ProjectsSort>,
    pub(super) pending: Memo<bool>,
    pub(super) ready: Memo<bool>,
    pub(super) error: Memo<Option<ViewerClientError>>,
    pub(super) select_page_size: Callback<ProjectsPageSize>,
    pub(super) select_sort: Callback<ProjectsSort>,
    pub(super) retry: Callback<()>,
}

pub(super) fn use_projects_presentation(active: Memo<bool>) -> ProjectsPresentation {
    let viewer = use_context::<ViewerContext>();
    let mut settings = use_resource(move || {
        let _ = viewer.server_instance_id();
        load_preferences(active() && viewer.actions_enabled())
    });
    let mut saving = use_signal(|| false);
    let mut save_error = use_signal(|| None);
    let mut attempted = use_signal(|| None);
    let preferences = use_memo(move || {
        settings
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(|settings| ProjectsPreferences {
                page_size: settings.projects_page_size,
                sort: settings.projects_sort,
            })
            .unwrap_or_default()
    });
    let sort = use_memo(move || preferences().sort);
    let page_size = use_memo(move || preferences().page_size);
    let ready = use_memo(move || settings.read().as_ref().is_some_and(Result::is_ok));
    let pending = use_memo(move || {
        saving()
            || (settings.state().cloned() == UseResourceState::Pending && settings.read().is_none())
            || !viewer.actions_enabled()
    });
    let error = use_memo(move || {
        save_error().or_else(|| {
            settings
                .read()
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .cloned()
        })
    });
    let mut save = use_action(move |request: EditSettingsRequest| async move {
        let instance_id = viewer.server_instance_id();
        let result = viewer_server::edit_settings(request).await;
        saving.set(false);
        if viewer.server_instance_id() != instance_id {
            return Ok::<(), std::convert::Infallible>(());
        }
        match result {
            Ok(()) => settings.restart(),
            Err(error) => save_error.set(Some(error)),
        }
        Ok(())
    });
    let update = use_callback(move |request: EditSettingsRequest| {
        if *pending.peek() {
            return;
        }
        attempted.set(Some(request.clone()));
        saving.set(true);
        save_error.set(None);
        save.call(request);
    });
    let select_sort = use_callback(move |sort: ProjectsSort| {
        update(EditSettingsRequest {
            projects_sort: FieldUpdate::Update(sort),
            ..Default::default()
        });
    });
    let select_page_size = use_callback(move |size: ProjectsPageSize| {
        update(EditSettingsRequest {
            projects_page_size: FieldUpdate::Update(size),
            ..Default::default()
        });
    });
    let retry = use_callback(move |()| {
        let request = attempted.peek().clone();
        if let Some(request) = request {
            update(request);
        } else {
            settings.restart();
        }
    });
    ProjectsPresentation {
        page_size,
        sort,
        pending,
        ready,
        error,
        select_page_size,
        select_sort,
        retry,
    }
}

async fn load_preferences(active: bool) -> Result<ViewerUserSettings, ViewerClientError> {
    if !active {
        std::future::pending::<()>().await;
    }
    viewer_server::get_settings().await
}
