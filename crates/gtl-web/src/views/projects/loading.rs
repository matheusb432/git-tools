use std::time::Duration;

use dioxus::prelude::*;
use gtl_wire::viewer::projects::ViewerProject;

use super::motion;
use crate::{
    app::application_layout::ViewerContext, entities::diffs::viewer_server,
    shared::viewer_client::ViewerClientError,
};

#[derive(Clone, Default, PartialEq)]
pub(super) struct ProjectLoad {
    pub(super) instance_id: Option<String>,
    pub(super) projects: Option<Vec<ViewerProject>>,
    pub(super) error: Option<ViewerClientError>,
    pub(super) refreshing: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Projects {
    pub(super) load: ReadSignal<ProjectLoad>,
    pub(super) refresh: Callback<()>,
}

pub(super) fn use_projects() -> Projects {
    let viewer = use_context::<ViewerContext>();
    let load = use_signal(ProjectLoad::default);
    use_future(move || poll_projects(load, viewer));
    use_effect(move || {
        if viewer.actions_enabled() {
            spawn(refresh_projects(load, viewer));
        }
    });
    let refresh = use_callback(move |()| {
        spawn(refresh_projects(load, viewer));
    });
    Projects {
        load: load.into(),
        refresh,
    }
}

async fn poll_projects(load: Signal<ProjectLoad>, viewer: ViewerContext) {
    loop {
        dioxus_sdk_time::sleep(Duration::from_secs(30)).await;
        if motion::visible() {
            refresh_projects(load, viewer).await;
        }
    }
}

async fn refresh_projects(mut load: Signal<ProjectLoad>, viewer: ViewerContext) {
    if !viewer.actions_enabled() || load.peek().refreshing {
        return;
    }
    let instance_id = viewer.server_instance_id();
    if load.peek().instance_id != instance_id {
        load.set(ProjectLoad {
            instance_id: instance_id.clone(),
            ..Default::default()
        });
    }
    load.write().refreshing = true;
    let result = viewer_server::list_projects().await;
    if viewer.server_instance_id() != instance_id {
        load.set(ProjectLoad::default());
        return;
    }
    let positions = motion::positions();
    let mut next = load.peek().clone();
    next.refreshing = false;
    match result {
        Ok(projects) => {
            next.projects = Some(projects);
            next.error = None;
        }
        Err(error) => next.error = Some(error),
    }
    load.set(next);
    motion::animate(positions).await;
}
