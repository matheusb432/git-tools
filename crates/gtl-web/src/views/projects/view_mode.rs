use dioxus::prelude::*;
use gtl_models::settings::ProjectsViewMode;
use gtl_wire::viewer::{EditSettingsRequest, FieldUpdate};
use lucide_dioxus::{LayoutGrid, List};

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::{
        ui::{Button, ButtonSize, ButtonState, ButtonVariant},
        viewer_client::ViewerClientError,
    },
};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ProjectsPresentation {
    pub(super) mode: Memo<ProjectsViewMode>,
    pub(super) pending: Memo<bool>,
    pub(super) error: Memo<Option<ViewerClientError>>,
    pub(super) select: Callback<ProjectsViewMode>,
    pub(super) retry: Callback<()>,
}

pub(super) fn use_projects_presentation() -> ProjectsPresentation {
    let viewer = use_context::<ViewerContext>();
    let mut settings = use_resource(move || {
        let _ = viewer.server_instance_id();
        viewer_server::get_settings()
    });
    let mut saving = use_signal(|| false);
    let mut save_error = use_signal(|| None);
    let mut attempted = use_signal(|| None);
    let mode = use_memo(move || {
        settings
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map_or(ProjectsViewMode::Grid, |settings| settings.projects_view)
    });
    let pending =
        use_memo(move || saving() || settings.read().is_none() || !viewer.actions_enabled());
    let error = use_memo(move || {
        save_error().or_else(|| {
            settings
                .read()
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .copied()
        })
    });
    let mut save = use_action(move |mode: ProjectsViewMode| async move {
        let instance_id = viewer.server_instance_id();
        let result = viewer_server::edit_settings(EditSettingsRequest {
            projects_view: FieldUpdate::Update(mode),
            ..Default::default()
        })
        .await;
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
    let select = use_callback(move |mode: ProjectsViewMode| {
        if *pending.peek() {
            return;
        }
        attempted.set(Some(mode));
        saving.set(true);
        save_error.set(None);
        save.call(mode);
    });
    let retry = use_callback(move |()| {
        if let Some(mode) = *attempted.peek() {
            select(mode);
        } else {
            settings.restart();
        }
    });
    ProjectsPresentation {
        mode,
        pending,
        error,
        select,
        retry,
    }
}

#[component]
pub(super) fn ProjectsViewToggle(presentation: ProjectsPresentation) -> Element {
    let mode = (presentation.mode)();
    rsx! {
        div {
            class: "project-view-toggle gap-px p-px",
            role: "group",
            aria_label: "Projects view",
            for (value, label) in [(ProjectsViewMode::Grid, "Grid view"), (ProjectsViewMode::Table, "List view")] {
                Button {
                    variant: if mode == value { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    size: ButtonSize::IconSmall,
                    class: "max-sm:size-11",
                    state: if (presentation.pending)() { ButtonState::Disabled } else { ButtonState::Enabled },
                    aria_label: label,
                    title: label,
                    aria_pressed: (mode == value).to_string(),
                    onclick: move |_| (presentation.select)(value),
                    if value == ProjectsViewMode::Grid {
                        LayoutGrid { size: 16 }
                    } else {
                        List { size: 16 }
                    }
                }
            }
        }
    }
}
