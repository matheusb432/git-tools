use dioxus::prelude::*;
use gtl_models::{paths::RepositoryRoot, viewer::ViewerTabId};
use gtl_wire::viewer::{ViewerTab, projects::OpenViewerProject};

use crate::{
    app::{
        application_layout::{ViewerContext, ViewerShellLoad},
        application_router::Route,
    },
    entities::diffs::viewer_server,
    shared::{
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{Button, ButtonVariant, PageNotice},
        viewer_client::ViewerClientError,
    },
};

struct ProjectDiffDestination {
    server_instance_id: String,
    request: OpenViewerProject,
    tab_id: ViewerTabId,
}

#[derive(Default)]
struct ProjectDiffDestinations {
    entries: Vec<ProjectDiffDestination>,
}

impl ProjectDiffDestinations {
    fn resolve(
        &mut self,
        server_instance_id: &str,
        request: &OpenViewerProject,
        tabs: &[ViewerTab],
    ) -> Option<ViewerTabId> {
        self.entries.retain(|entry| {
            entry.server_instance_id == server_instance_id
                && tabs.iter().any(|tab| tab.id == entry.tab_id)
        });
        self.entries
            .iter()
            .find(|entry| entry.request == *request)
            .map(|entry| entry.tab_id)
    }

    fn record(
        &mut self,
        server_instance_id: String,
        request: OpenViewerProject,
        tab_id: ViewerTabId,
    ) {
        self.entries.retain(|entry| {
            entry.server_instance_id == server_instance_id && entry.request != request
        });
        self.entries.push(ProjectDiffDestination {
            server_instance_id,
            request,
            tab_id,
        });
    }
}

pub(crate) fn use_project_diff_destinations_provider() {
    let destinations = use_signal(ProjectDiffDestinations::default);
    use_context_provider(|| destinations);
}

#[component]
pub(crate) fn ProjectDiffView(path: Option<RepositoryRoot>) -> Element {
    let language = use_language();
    let mut opening = use_project_diff(path.as_ref());
    rsx! {
        main { class: "h-full",
            match &*opening.read() {
                Some(Err(error)) => rsx! {
                    PageNotice {
                        class: "h-full",
                        role: "alert",
                        title: t!(language, "project-diff-failed"),
                        message: client_error_message(error, language),
                        Button { variant: ButtonVariant::Outline, onclick: move |_| opening.restart(),
                            {t!(language, "action-try-again")}
                        }
                    }
                },
                _ => rsx! {
                    PageNotice {
                        class: "h-full",
                        role: "status",
                        title: t!(language, "project-diff-opening"),
                        message: t!(language, "project-diff-loading"),
                    }
                },
            }
        }
    }
}

fn use_project_diff(path: Option<&RepositoryRoot>) -> Resource<Result<(), ViewerClientError>> {
    let viewer = use_context::<ViewerContext>();
    let destinations = use_context::<Signal<ProjectDiffDestinations>>();
    let navigator = use_navigator();
    let path = path.cloned();
    use_resource(use_reactive((&path,), move |(path,)| {
        open_project_diff(path, viewer, navigator, destinations)
    }))
}

/// Shows the project's open tab, or opens one; an open tab updates only when asked.
async fn open_project_diff(
    path: Option<RepositoryRoot>,
    viewer: ViewerContext,
    navigator: dioxus::router::Navigator,
    mut destinations: Signal<ProjectDiffDestinations>,
) -> Result<(), ViewerClientError> {
    let Some(instance) = viewer.server_instance_id() else {
        return Ok(());
    };
    if !viewer.actions_enabled() {
        return Ok(());
    }
    let path = path.ok_or(ViewerClientError::InvalidMessage)?;
    let request = OpenViewerProject { path };
    let cached = match &*viewer.shell().peek() {
        ViewerShellLoad::Ready(shell) => {
            destinations
                .write()
                .resolve(&instance, &request, &shell.tabs)
        }
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => None,
    };
    if let Some(tab_id) = cached {
        navigator.replace(Route::Diff { tab_id });
        return Ok(());
    }
    let opened: Result<_, ViewerClientError> = async {
        let opened = viewer_server::open_project(request.clone()).await?;
        let shell = viewer_server::get_shell().await?;
        Ok((opened.tab_id, shell))
    }
    .await;
    if viewer.server_instance_id().as_deref() != Some(instance.as_str()) {
        return Ok(());
    }
    let (tab_id, shell) = opened?;
    viewer.replace_shell(shell);
    destinations.write().record(instance, request, tab_id);
    navigator.replace(Route::Diff { tab_id });
    Ok(())
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerTabState;

    use super::*;
    use crate::test_support::{TestResult, recipe_label, viewer_tab_id};

    fn request(path: &str) -> TestResult<OpenViewerProject> {
        Ok(OpenViewerProject {
            path: RepositoryRoot::try_new(path.into())?,
        })
    }

    fn tab(id: u64) -> TestResult<ViewerTab> {
        Ok(ViewerTab {
            details: None,
            custom_name: None,
            pinned: false,
            id: viewer_tab_id(id)?,
            label: recipe_label("Project")?,
            live: false,
            state: ViewerTabState::Ready,
        })
    }

    #[test]
    fn destinations_resolve_by_repository() -> TestResult {
        let mut destinations = ProjectDiffDestinations::default();
        let alpha = request("/tmp/alpha")?;
        let beta = request("/tmp/beta")?;
        let tabs = [tab(1)?, tab(2)?];
        destinations.record("server".to_owned(), alpha.clone(), tabs[0].id);
        destinations.record("server".to_owned(), beta.clone(), tabs[1].id);
        assert_eq!(
            destinations.resolve("server", &alpha, &tabs),
            Some(tabs[0].id)
        );
        assert_eq!(
            destinations.resolve("server", &beta, &tabs),
            Some(tabs[1].id)
        );
        assert_eq!(
            destinations.resolve("server", &request("/tmp/gamma")?, &tabs),
            None
        );
        Ok(())
    }

    #[test]
    fn closed_destinations_are_discarded_before_reopening() -> TestResult {
        let mut destinations = ProjectDiffDestinations::default();
        let request = request("/tmp/alpha")?;
        let closed = tab(1)?;
        let remaining = tab(2)?;
        destinations.record("server".to_owned(), request.clone(), closed.id);
        assert_eq!(destinations.resolve("server", &request, &[remaining]), None);
        assert_eq!(destinations.resolve("server", &request, &[closed]), None);

        let reopened = tab(3)?;
        destinations.record("server".to_owned(), request.clone(), reopened.id);
        assert_eq!(
            destinations.resolve("server", &request, std::slice::from_ref(&reopened)),
            Some(reopened.id)
        );
        Ok(())
    }

    #[test]
    fn server_replacement_discards_destinations_even_when_tab_ids_match() -> TestResult {
        let mut destinations = ProjectDiffDestinations::default();
        let request = request("/tmp/alpha")?;
        let tabs = [tab(1)?];
        destinations.record("old".to_owned(), request.clone(), tabs[0].id);
        assert_eq!(destinations.resolve("new", &request, &tabs), None);
        assert_eq!(destinations.resolve("old", &request, &tabs), None);
        destinations.record("new".to_owned(), request.clone(), tabs[0].id);
        assert_eq!(
            destinations.resolve("new", &request, &tabs),
            Some(tabs[0].id)
        );
        Ok(())
    }
}
