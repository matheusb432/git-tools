use dioxus::prelude::*;
use gtl_models::{paths::RepositoryRoot, viewer::ViewerTabId};
use gtl_wire::viewer::{
    ViewerTab,
    projects::{OpenViewerProject, ViewerProjectDiffMode},
};

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
pub(crate) fn ProjectDiffView(
    path: Option<RepositoryRoot>,
    mode: ViewerProjectDiffMode,
) -> Element {
    let language = use_language();
    let mut opening = use_project_diff(path.as_ref(), mode);
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

fn use_project_diff(
    path: Option<&RepositoryRoot>,
    mode: ViewerProjectDiffMode,
) -> Resource<Result<(), ViewerClientError>> {
    let viewer = use_context::<ViewerContext>();
    let destinations = use_context::<Signal<ProjectDiffDestinations>>();
    let navigator = use_navigator();
    let path = path.cloned();
    use_resource(use_reactive((&path, &mode), move |(path, mode)| {
        open_project_diff(path, mode, viewer, navigator, destinations)
    }))
}

async fn open_project_diff(
    path: Option<RepositoryRoot>,
    mode: ViewerProjectDiffMode,
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
    let request = OpenViewerProject { path, mode };
    let cached = match &*viewer.shell().peek() {
        ViewerShellLoad::Ready(shell) => {
            destinations
                .write()
                .resolve(&instance, &request, &shell.tabs)
        }
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => None,
    };
    if request.mode == ViewerProjectDiffMode::Live
        && let Some(tab_id) = cached
    {
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
    use gtl_wire::viewer::{ViewerTabKind, ViewerTabState};

    use super::*;
    use crate::test_support::{TestResult, recipe_label, viewer_tab_id};

    fn request(path: &str, mode: ViewerProjectDiffMode) -> TestResult<OpenViewerProject> {
        Ok(OpenViewerProject {
            path: RepositoryRoot::try_new(path.into())?,
            mode,
        })
    }

    fn tab(id: u64) -> TestResult<ViewerTab> {
        Ok(ViewerTab {
            custom_name: None,
            pinned: false,
            id: viewer_tab_id(id)?,
            label: recipe_label("Project")?,
            kind: ViewerTabKind::Snapshot,
            state: ViewerTabState::Ready,
        })
    }

    #[test]
    fn destinations_resolve_by_repository_and_mode() -> TestResult {
        let mut destinations = ProjectDiffDestinations::default();
        let local = request("/tmp/alpha", ViewerProjectDiffMode::Snapshot)?;
        let unpushed = request("/tmp/alpha", ViewerProjectDiffMode::Live)?;
        let other = request("/tmp/beta", ViewerProjectDiffMode::Snapshot)?;
        let tabs = [tab(1)?, tab(2)?, tab(3)?];
        for (request, tab) in [
            (&local, &tabs[0]),
            (&unpushed, &tabs[1]),
            (&other, &tabs[2]),
        ] {
            destinations.record("server".to_owned(), request.clone(), tab.id);
        }
        assert_eq!(
            destinations.resolve("server", &local, &tabs),
            Some(tabs[0].id)
        );
        assert_eq!(
            destinations.resolve("server", &unpushed, &tabs),
            Some(tabs[1].id)
        );
        assert_eq!(
            destinations.resolve("server", &other, &tabs),
            Some(tabs[2].id)
        );
        assert_eq!(
            destinations.resolve(
                "server",
                &request("/tmp/beta", ViewerProjectDiffMode::Live)?,
                &tabs,
            ),
            None
        );
        Ok(())
    }

    #[test]
    fn closed_destinations_are_discarded_before_reopening() -> TestResult {
        let mut destinations = ProjectDiffDestinations::default();
        let request = request("/tmp/alpha", ViewerProjectDiffMode::Snapshot)?;
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
        let request = request("/tmp/alpha", ViewerProjectDiffMode::Snapshot)?;
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
