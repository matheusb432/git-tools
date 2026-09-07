use dioxus::prelude::*;
use gtl_models::{live_views::LiveComparison, paths::RepositoryRoot};
use gtl_wire::viewer::projects::OpenViewerProject;

use crate::{
    app::{application_layout::ViewerContext, application_router::Route},
    entities::diffs::viewer_server,
    shared::{
        ui::{Button, ButtonVariant, PageNotice},
        viewer_client::ViewerClientError,
    },
};

#[component]
pub(crate) fn ProjectDiffView(path: Option<RepositoryRoot>, comparison: LiveComparison) -> Element {
    let mut opening = use_project_diff(path.as_ref(), comparison);
    rsx! {
        main { class: "h-full",
            match &*opening.read() {
                Some(Err(error)) => rsx! {
                    PageNotice {
                        class: "h-full",
                        role: "alert",
                        title: "Could not open comparison",
                        message: error.message(),
                        Button { variant: ButtonVariant::Outline, onclick: move |_| opening.restart(), "Try again" }
                    }
                },
                _ => rsx! {
                    PageNotice {
                        class: "h-full",
                        role: "status",
                        title: "Opening comparison",
                        message: "Loading diff...",
                    }
                },
            }
        }
    }
}

fn use_project_diff(
    path: Option<&RepositoryRoot>,
    comparison: LiveComparison,
) -> Resource<Result<(), ViewerClientError>> {
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let path = path.cloned();
    use_resource(use_reactive(
        (&path, &comparison),
        move |(path, comparison)| open_project_diff(path, comparison, viewer, navigator),
    ))
}

async fn open_project_diff(
    path: Option<RepositoryRoot>,
    comparison: LiveComparison,
    viewer: ViewerContext,
    navigator: dioxus::router::Navigator,
) -> Result<(), ViewerClientError> {
    let instance = viewer.server_instance_id();
    if !viewer.actions_enabled() {
        return Ok(());
    }
    let path = path.ok_or(ViewerClientError::InvalidRequest)?;
    let opened = viewer_server::open_project(OpenViewerProject { path, comparison }).await?;
    let shell = viewer_server::get_shell().await?;
    if viewer.server_instance_id() == instance {
        viewer.replace_shell(shell);
        navigator.replace(Route::Diff {
            tab_id: opened.tab_id,
        });
    }
    Ok(())
}
