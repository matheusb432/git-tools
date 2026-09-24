use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::settings::{ProjectsPageSize, ProjectsSort};
use gtl_wire::viewer::projects::{
    ListViewerProjects, ViewerProject, ViewerProjectPage, ViewerProjectSelection,
    ViewerProjectStatusUpdate, ViewerProjectsCursor, ViewerProjectsPageSize,
};

use crate::{
    app::application_layout::ViewerContext, entities::diffs::viewer_server,
    shared::viewer_client::ViewerClientError,
};

#[derive(Clone, PartialEq)]
pub(super) struct ProjectPageLoad {
    cursor: ViewerProjectsCursor,
    page_size: ProjectsPageSize,
    sort: ProjectsSort,
    pub(super) instance_id: Option<String>,
    pub(super) result: Result<ViewerProjectPage, ViewerClientError>,
}

#[derive(Clone, Copy)]
pub(super) struct Projects {
    pub(super) page: Memo<Option<ProjectPageLoad>>,
    pub(super) loading_page: Memo<bool>,
    pub(super) active: Memo<bool>,
    pub(super) refresh: Callback<()>,
    reload: Callback<()>,
    revision: ReadSignal<u64>,
}

pub(super) fn use_projects(
    cursor: Memo<ViewerProjectsCursor>,
    page_size: Memo<ProjectsPageSize>,
    sort: Memo<ProjectsSort>,
    active: Memo<bool>,
    ready: Memo<bool>,
) -> Projects {
    let viewer = use_context::<ViewerContext>();
    let push = use_context::<crate::views::push::PushController>();
    let mut revision = use_signal(|| 0_u64);
    let mut resource = use_resource(move || {
        let _ = revision();
        let _ = (push.refresh_epoch)();
        load_page(cursor(), page_size(), sort(), viewer, active() && ready())
    });
    let page = use_memo(move || resource.read().clone());
    let loading_page = use_memo(move || {
        resource.state().cloned() == UseResourceState::Pending
            && resource.read().as_ref().is_none_or(|loaded| {
                loaded.cursor != cursor()
                    || loaded.page_size != page_size()
                    || loaded.sort != sort()
            })
    });
    let refresh = use_callback(move |()| {
        if active() && viewer.actions_enabled() {
            revision.with_mut(|revision| *revision = revision.wrapping_add(1));
        }
    });
    let reload = use_callback(move |()| {
        if active() && viewer.actions_enabled() {
            resource.restart();
        }
    });
    use_retained_status_page(page);
    Projects {
        page,
        loading_page,
        active,
        refresh,
        reload,
        revision: revision.into(),
    }
}

pub(super) fn use_projects_active(route_active: Memo<bool>) -> Memo<bool> {
    let document_visible = crate::shared::browser::use_document_visible();
    let window_visible = use_signal(|| false);
    use_resource(move || observe_window_visibility(window_visible, route_active()));
    use_memo(move || route_active() && document_visible() && window_visible())
}

fn use_retained_status_page(page: Memo<Option<ProjectPageLoad>>) {
    let viewer = use_context::<ViewerContext>();
    let mut cache = use_context::<Signal<super::cache::ProjectStatusCache>>();
    use_effect(move || {
        let instance = viewer.server_instance_id();
        let page = page.read();
        let projects = page
            .as_ref()
            .filter(|load| load.instance_id == instance)
            .and_then(|load| load.result.as_ref().ok())
            .map_or(&[][..], ViewerProjectPage::projects);
        cache.write().retain_page(instance, projects);
    });
    use_drop(move || {
        if let Ok(mut cache) = cache.try_write() {
            cache.release_page();
        }
    });
}

#[component]
pub(super) fn ProjectsActivity() -> Element {
    let projects = use_context::<Projects>();
    use_future(move || poll_projects(projects.reload));
    use_status_watch(projects.page, projects.revision);
    rsx! {}
}

#[derive(Clone, Copy)]
pub(super) struct ProjectStatusHandle {
    pub(super) state: Memo<super::cache::ProjectStatusLoad>,
    pub(super) retry: Callback<()>,
}

pub(super) fn use_project_status(project: &ViewerProject) -> ProjectStatusHandle {
    let projects = use_context::<Projects>();
    let viewer = use_context::<ViewerContext>();
    let cache = use_context::<Signal<super::cache::ProjectStatusCache>>();
    let state = use_memo(use_reactive((project,), move |(project,)| {
        cache
            .read()
            .get(&project, viewer.server_instance_id().as_deref())
    }));
    ProjectStatusHandle {
        state,
        retry: projects.refresh,
    }
}

fn use_status_watch(page: Memo<Option<ProjectPageLoad>>, revision: ReadSignal<u64>) {
    let push = use_context::<crate::views::push::PushController>();
    let viewer = use_context::<ViewerContext>();
    let cache = use_context::<Signal<super::cache::ProjectStatusCache>>();
    let selected = use_memo(move || {
        page.read()
            .as_ref()
            .filter(|load| load.instance_id == viewer.server_instance_id())
            .and_then(|load| load.result.as_ref().ok())
            .map(|page| page.projects().to_vec())
            .unwrap_or_default()
    });
    use_resource(move || {
        let _ = revision();
        let _ = (push.refresh_epoch)();
        let instance = viewer.server_instance_id();
        let projects = if viewer.actions_enabled() {
            selected()
        } else {
            Vec::new()
        };
        watch_statuses(projects, instance, cache)
    });
}

async fn observe_window_visibility(mut visible: Signal<bool>, active: bool) {
    if !active {
        if *visible.peek() {
            visible.set(false);
        }
        return;
    }
    loop {
        if let Ok(state) = gtl_client::window::state().await
            && *visible.peek() != state.visible
        {
            visible.set(state.visible);
        }
        dioxus_sdk_time::sleep(Duration::from_millis(500)).await;
    }
}

async fn watch_statuses(
    projects: Vec<ViewerProject>,
    instance: Option<String>,
    mut cache: Signal<super::cache::ProjectStatusCache>,
) {
    if projects.is_empty() {
        return;
    }
    cache.write().retain_page(instance.clone(), &projects);
    let mut ids = projects
        .iter()
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    let selection = ViewerProjectSelection::try_from(ids);
    let Ok(selection) = selection else {
        return;
    };
    let mut retry = crate::shared::retry_delay::RetryDelay::default();
    for _ in 0..4 {
        let observed = projects.clone();
        let request = gtl_wire::viewer::WatchViewer {
            live_tab_id: None,
            projects: selection.clone(),
        };
        let result = viewer_server::listen_for_state_changes(
            request,
            |_| {},
            move |event| observe_status(cache, &observed, event),
        )
        .await;
        if result.is_err() {
            mark_statuses_unavailable(cache, &projects);
        }
        dioxus_sdk_time::sleep(retry.take_and_advance()).await;
    }
}

async fn load_page(
    cursor: ViewerProjectsCursor,
    page_size: ProjectsPageSize,
    sort: ProjectsSort,
    viewer: ViewerContext,
    active: bool,
) -> ProjectPageLoad {
    let instance_id = viewer.server_instance_id();
    if !active || !viewer.actions_enabled() {
        std::future::pending::<()>().await;
    }
    let result = query_page(cursor.clone(), page_size, sort).await;
    ProjectPageLoad {
        cursor,
        page_size,
        sort,
        instance_id,
        result,
    }
}

async fn query_page(
    cursor: ViewerProjectsCursor,
    page_size: ProjectsPageSize,
    sort: ProjectsSort,
) -> Result<ViewerProjectPage, ViewerClientError> {
    let request = ListViewerProjects {
        cursor,
        sort: Some(sort),
        page_size: ViewerProjectsPageSize::try_new(page_size.into_inner())
            .map_err(|_| ViewerClientError::InvalidMessage)?,
    };
    let page = viewer_server::list_projects(request.clone()).await?;
    if page.projects().is_empty()
        && page.total() > 0
        && request.cursor != ViewerProjectsCursor::First
    {
        viewer_server::list_projects(ListViewerProjects {
            cursor: if matches!(request.cursor, ViewerProjectsCursor::Before(_)) {
                ViewerProjectsCursor::First
            } else {
                ViewerProjectsCursor::Last
            },
            ..request
        })
        .await
    } else {
        Ok(page)
    }
}

async fn poll_projects(refresh: Callback<()>) {
    loop {
        dioxus_sdk_time::sleep(Duration::from_secs(30)).await;
        refresh(());
    }
}

fn observe_status(
    mut cache: Signal<super::cache::ProjectStatusCache>,
    projects: &[ViewerProject],
    event: gtl_wire::viewer::ViewerStateChanged,
) {
    if let Some(update) = event.project_status
        && cache.peek().needs_update(projects, &update)
    {
        cache.write().observe(projects, update);
    }
}

fn mark_statuses_unavailable(
    mut cache: Signal<super::cache::ProjectStatusCache>,
    projects: &[ViewerProject],
) {
    for project in projects {
        cache.write().observe(
            projects,
            ViewerProjectStatusUpdate::Unavailable(project.id.clone()),
        );
    }
}
