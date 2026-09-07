mod motion;

use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::{
    live_views::LiveComparison,
    repository::status::{
        RepositoryStatus, StatusChanges, StatusClass, StatusHead, StatusUpstream,
    },
};
use gtl_wire::viewer::projects::{OpenViewerProject, ViewerProject};
use lucide_dioxus::{ArrowUp, FileDiff, GitBranch, LayoutGrid, RefreshCw};

use crate::{
    app::{application_layout::ViewerContext, application_router::Route},
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{Button, ButtonSize, ButtonState, ButtonVariant, PageNotice, ScrollArea, Skeleton},
        viewer_client::ViewerClientError,
    },
};

#[derive(Clone, Default, PartialEq)]
struct ProjectLoad {
    instance_id: Option<String>,
    projects: Option<Vec<ViewerProject>>,
    error: Option<ViewerClientError>,
    refreshing: bool,
}

#[derive(Clone, Copy)]
struct Projects {
    load: ReadSignal<ProjectLoad>,
    refresh: Callback<()>,
}

fn use_projects() -> Projects {
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

#[component]
pub(crate) fn ProjectsView() -> Element {
    let projects = use_projects();
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let mut opening = use_signal(|| None::<OpenViewerProject>);
    let mut error = use_signal(|| None::<ViewerClientError>);
    let mut retry = use_signal(|| None::<OpenViewerProject>);
    let mut open = use_action(move |request: OpenViewerProject| async move {
        let instance_id = viewer.server_instance_id();
        retry.set(Some(request.clone()));
        let result = viewer_server::open_project(request).await;
        opening.set(None);
        if viewer.server_instance_id() == instance_id {
            match result {
                Ok(result) => {
                    if let Ok(shell) = viewer_server::get_shell().await {
                        viewer.replace_shell(shell);
                    }
                    navigator.push(Route::Diff {
                        tab_id: result.tab_id,
                    });
                }
                Err(next) => error.set(Some(next)),
            }
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let open_project = use_callback(move |request: OpenViewerProject| {
        if opening.peek().is_some() || !viewer.actions_enabled() {
            return;
        }
        error.set(None);
        opening.set(Some(request.clone()));
        open.call(request);
    });
    let try_again = use_callback(move |()| {
        if error.peek().is_some() {
            if let Some(request) = retry.peek().clone() {
                open_project(request);
            }
        } else {
            (projects.refresh)(());
        }
    });
    use_effect(move || browser::focus_element("projects-heading".into()));
    let load = projects.load.read();
    let count = load.projects.as_ref().map(Vec::len);
    let pending = opening();
    let disabled = pending.is_some() || !viewer.actions_enabled();
    rsx! {
        document::Title { "Projects - git-tools" }
        main {
            class: "flex h-full min-h-0 flex-col overflow-hidden",
            "data-testid": "projects-view",
            header { class: "flex shrink-0 items-center justify-between gap-4 border-b border-line px-5 py-5 sm:px-8 sm:py-7",
                div {
                    p { class: "mb-2 flex items-center gap-2 font-mono text-xs tracking-widest text-ink-3 uppercase",
                        LayoutGrid { size: 14 }
                        "Your workspace"
                    }
                    h1 {
                        id: "projects-heading",
                        tabindex: "-1",
                        class: "text-2xl font-semibold tracking-tight text-ink focus:outline-none",
                        "Projects"
                    }
                    p { class: "mt-2 text-ink-2", "Review local changes and unpushed commits." }
                }
                Button {
                    variant: ButtonVariant::Outline,
                    state: if load.refreshing { ButtonState::Loading } else if viewer.actions_enabled() { ButtonState::Enabled } else { ButtonState::Disabled },
                    onclick: move |_| (projects.refresh)(()),
                    aria_label: "Refresh projects",
                    icon: rsx! {
                        RefreshCw { size: 14 }
                    },
                    span { class: "hidden sm:inline", "Refresh" }
                }
            }
            ScrollArea { class: "min-h-0 flex-1 overflow-auto px-5 py-5 sm:px-8 sm:py-6",
                div { class: "mx-auto max-w-7xl",
                    if let Some(error) = error().or(load.error) {
                        div {
                            class: "mb-5 rounded-sm border border-warn-line bg-warn-bg p-3 text-warn",
                            role: "alert",
                            "{error.message()} "
                            if load.projects.is_some() {
                                "Showing the last known status."
                            }
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Small,
                                onclick: move |_| try_again(()),
                                "Try again"
                            }
                        }
                    }
                    div { class: "mb-5 flex items-center justify-between gap-3 font-mono text-xs text-ink-3",
                        if let Some(count) = count {
                            span { "{count} projects" }
                        } else {
                            span { "Loading projects" }
                        }
                        span { "Changes to review first" }
                    }
                    match &load.projects {
                        None if load.error.is_some() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: "Projects unavailable",
                                message: "Check the project catalogue and try Refresh.",
                            }
                        },
                        None => rsx! {
                            div {
                                class: "grid grid-cols-1 gap-5 md:grid-cols-2 xl:grid-cols-3",
                                aria_label: "Loading projects",
                                for index in 0..6 {
                                    Skeleton { key: "{index}", class: "h-60 rounded-panel" }
                                }
                            }
                        },
                        Some(items) if items.is_empty() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: "No managed projects",
                                message: "Projects managed in sample_project appear here.",
                            }
                        },
                        Some(items) => rsx! {
                            div {
                                class: "grid grid-cols-1 gap-5 md:grid-cols-2 xl:grid-cols-3",
                                aria_label: "Managed projects",
                                for project in items {
                                    ProjectCard {
                                        key: "{project.path}",
                                        project: project.clone(),
                                        disabled,
                                        opening: pending.clone(),
                                        open: open_project,
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectCard(
    project: ViewerProject,
    disabled: bool,
    opening: Option<OpenViewerProject>,
    open: Callback<OpenViewerProject>,
) -> Element {
    let (branch, changes, ahead, issue) = match &project.status {
        RepositoryStatus::Absent => (
            "Unavailable",
            StatusChanges::Unavailable,
            None,
            Some("Repository not found"),
        ),
        RepositoryStatus::Present { head, changes } => {
            let (branch, ahead, issue) = match head {
                StatusHead::Unavailable => ("Unavailable", None, Some("Branch status unavailable")),
                StatusHead::Detached => ("Detached HEAD", None, Some("No upstream configured")),
                StatusHead::Branch { name, upstream } => match upstream {
                    StatusUpstream::Missing => {
                        (name.as_str(), None, Some("No upstream configured"))
                    }
                    StatusUpstream::Tracking { ahead, .. } => {
                        (name.as_str(), Some(ahead.into_inner()), None)
                    }
                },
            };
            let issue = if matches!(changes, StatusChanges::Unavailable) {
                Some("Working-tree status unavailable")
            } else {
                issue
            };
            (branch, *changes, ahead, issue)
        }
    };
    let local_available = !matches!(changes, StatusChanges::Unavailable);
    let symbols = changes.symbols();
    let symbols_description = match changes {
        StatusChanges::Clean => "Working tree clean",
        StatusChanges::Unavailable => "Working-tree status unavailable",
        StatusChanges::Changed { tracked, untracked } => {
            match (!tracked.is_zero(), !untracked.is_zero()) {
                (true, true) => "Modified and untracked files",
                (true, false) => "Modified files",
                (false, true) => "Untracked files",
                (false, false) => "Working tree clean",
            }
        }
    };
    let (status_label, status_color) = match project.status.class() {
        StatusClass::Pending => ("Changes to review", "text-acc"),
        StatusClass::Clean => ("Up to date", "text-add"),
        StatusClass::Warn | StatusClass::Absent => {
            (issue.unwrap_or("Status unavailable"), "text-warn")
        }
    };
    let commits = ahead.map_or_else(|| "-".to_owned(), |count| count.to_string());
    let rendered = project.last_rendered_at.as_ref().map_or_else(
        || "Not rendered yet".to_owned(),
        |time| format!("Rendered {}", time.display_minute()),
    );
    rsx! {
        article {
            class: "flex min-w-0 flex-col rounded-panel border border-line bg-surface p-5 transition-[border-color,box-shadow] duration-150 hover:border-line-2 motion-reduce:transition-none",
            "data-project-card": "{project.path}",
            aria_label: "{project.name}",
            div { class: "flex min-w-0 items-start justify-between gap-3",
                h2 {
                    class: "min-w-0 truncate text-lg font-semibold tracking-tight text-ink",
                    title: "{project.path}",
                    "{project.name}"
                }
                span { class: "shrink-0 font-mono text-[10px] {status_color}", "{status_label}" }
            }
            p { class: "mt-2 flex min-w-0 items-center gap-1.5 font-mono text-xs text-ink-3",
                GitBranch { size: 13 }
                span { class: "truncate", title: branch, "{branch}" }
                span {
                    class: "shrink-0 {status_color}",
                    role: "img",
                    title: symbols_description,
                    aria_label: symbols_description,
                    "[{symbols}]"
                }
            }
            p { class: "mt-5 flex items-baseline gap-2 border-y border-line py-3 text-xs text-ink-2",
                span { class: "font-mono text-lg font-medium tabular-nums text-ink",
                    "{commits}"
                }
                "Unpushed commits"
            }
            div { class: "mt-5 grid grid-cols-1 gap-2 min-[420px]:grid-cols-2",
                for comparison in [LiveComparison::LocalChanges, LiveComparison::UnpushedCommits] {
                    ProjectComparisonAction {
                        request: OpenViewerProject {
                            path: project.path.clone(),
                            comparison,
                        },
                        available: match comparison {
                            LiveComparison::LocalChanges => local_available,
                            LiveComparison::UnpushedCommits => ahead.is_some(),
                        },
                        disabled,
                        pending: opening
                            .as_ref()
                            .is_some_and(|request| {
                                request.path == project.path && request.comparison == comparison
                            }),
                        issue,
                        open,
                    }
                }
            }

            p {
                class: "mt-4 truncate font-mono text-[10px] text-ink-3",
                title: rendered.clone(),
                "{rendered}"
            }
        }
    }
}

#[component]
fn ProjectComparisonAction(
    request: OpenViewerProject,
    available: bool,
    disabled: bool,
    pending: bool,
    issue: Option<&'static str>,
    open: Callback<OpenViewerProject>,
) -> Element {
    let comparison = request.comparison;
    let state = if pending {
        ButtonState::Loading
    } else if disabled || !available {
        ButtonState::Disabled
    } else {
        ButtonState::Enabled
    };
    let title = if available {
        comparison.label()
    } else {
        issue.unwrap_or("Status unavailable")
    };
    rsx! {
        Button {
            variant: ButtonVariant::Outline,
            size: ButtonSize::Small,
            class: "min-h-9",
            state,
            title,
            onclick: move |_| open(request.clone()),
            if comparison == LiveComparison::LocalChanges {
                FileDiff { size: 13 }
            } else {
                ArrowUp { size: 13 }
            }
            "{comparison.label()}"
        }
    }
}
