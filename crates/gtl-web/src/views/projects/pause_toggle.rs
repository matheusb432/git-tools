use dioxus::prelude::*;
use gtl_models::projects::catalogue::ProjectStatus;
use gtl_wire::viewer::projects::{SetViewerProjectStatus, ViewerProject};
use lucide_dioxus::{CirclePause, CirclePlay};

use super::loading::Projects;
use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonState, ButtonVariant, use_toast},
    },
};

/// Pauses an active project or resumes a paused one.
#[component]
pub(super) fn ProjectPauseToggle(project: ViewerProject, disabled: bool) -> Element {
    let language = use_language();
    let toast = use_toast();
    let projects = use_context::<Projects>();
    let mut toggle = use_action(
        move |request: (SetViewerProjectStatus, String)| async move {
            let (request, project) = request;
            let status = request.status;
            match viewer_server::set_project_status(request).await {
                Ok(()) => {
                    if !(projects.status)().includes(status) {
                        browser::focus_element("projects-heading".to_owned());
                    }
                    (projects.refresh)(());
                    toast.ok(match status {
                        ProjectStatus::Active => {
                            t!(language, "projects-resumed", project = project)
                        }
                        ProjectStatus::Paused => t!(language, "projects-paused", project = project),
                    });
                }
                Err(error) => toast.client_error(&error),
            }
            Ok::<(), std::convert::Infallible>(())
        },
    );
    let (target, label) = match project.status {
        ProjectStatus::Active => (
            ProjectStatus::Paused,
            t!(
                language,
                "projects-pause",
                project = project.name.to_string()
            ),
        ),
        ProjectStatus::Paused => (
            ProjectStatus::Active,
            t!(
                language,
                "projects-resume",
                project = project.name.to_string()
            ),
        ),
    };
    let state = if toggle.pending() {
        ButtonState::Loading
    } else if disabled {
        ButtonState::Disabled
    } else {
        ButtonState::Enabled
    };
    rsx! {
        Button {
            id: format!("project-pause-{}", project.id),
            variant: ButtonVariant::Ghost,
            size: ButtonSize::IconSmall,
            state,
            aria_label: label.clone(),
            title: label,
            onclick: move |_| {
                toggle
                    .call((
                        SetViewerProjectStatus {
                            project_id: project.id.clone(),
                            status: target,
                        },
                        project.name.to_string(),
                    ));
            },
            if !toggle.pending() {
                match project.status {
                    ProjectStatus::Active => rsx! {
                        CirclePause { size: 15 }
                    },
                    ProjectStatus::Paused => rsx! {
                        CirclePlay { size: 15 }
                    },
                }
            }
        }
    }
}
