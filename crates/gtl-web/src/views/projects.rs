mod motion;

use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::{
    live_views::LiveComparison,
    paths::RepositoryRoot,
    repository::status::{
        RepositoryStatus, StatusChanges, StatusClass, StatusHead, StatusUpstream,
    },
    settings::ProjectsViewMode,
};
use gtl_wire::viewer::{EditSettingsRequest, FieldUpdate, projects::ViewerProject};
use lucide_dioxus::{ArrowUp, FileDiff, GitBranch, LayoutGrid, List, RefreshCw};

use crate::{
    app::{application_layout::ViewerContext, application_router::Route},
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{
            Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant, PageNotice, ScrollArea,
            Skeleton, button_classes, no_data::NoData,
        },
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
    let presentation = use_projects_presentation();
    let viewer = use_context::<ViewerContext>();
    let try_again = use_callback(move |()| {
        if (presentation.error)().is_some() {
            (presentation.retry)(());
        } else {
            (projects.refresh)(());
        }
    });
    use_effect(move || browser::focus_element("projects-heading".into()));
    let load = projects.load.read();
    let count = load.projects.as_ref().map(Vec::len);
    let mode = (presentation.mode)();
    let disabled = !viewer.actions_enabled();
    rsx! {
        document::Title { "Projects - git-tools" }
        main {
            class: "flex h-full min-h-0 flex-col overflow-hidden",
            "data-testid": "projects-view",
            header { class: "flex shrink-0 flex-wrap items-center justify-between gap-4 border-b border-line px-5 py-5 sm:px-8 sm:py-7",
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
                div { class: "flex shrink-0 items-center gap-3",
                    ProjectsViewToggle { presentation }
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
            }
            ScrollArea { class: "min-h-0 flex-1 overflow-auto px-5 py-5 sm:px-8 sm:py-6",
                div { class: "mx-auto max-w-7xl",
                    if let Some(error) = load.error.or((presentation.error)()) {
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
                                message: "Projects managed in Git Tools appear here.",
                            }
                        },
                        Some(items) if mode == ProjectsViewMode::Table => rsx! {
                            ProjectTable { projects: items.clone(), disabled }
                        },
                        Some(items) => rsx! {
                            div {
                                class: "grid grid-cols-1 gap-5 md:grid-cols-2 xl:grid-cols-3",
                                aria_label: "Managed projects",
                                for project in items {
                                    ProjectCard { key: "{project.path}", project: project.clone(), disabled }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

struct ProjectStatusPresentation<'a> {
    branch: Option<&'a str>,
    local_available: bool,
    ahead: Option<u64>,
    issue: Option<&'static str>,
    symbols: String,
    symbols_description: &'static str,
    status_label: &'static str,
    status_color: &'static str,
}

fn project_status(status: &RepositoryStatus) -> ProjectStatusPresentation<'_> {
    let (branch, changes, ahead, issue) = match status {
        RepositoryStatus::Absent => (
            None,
            StatusChanges::Unavailable,
            None,
            Some("Repository not found"),
        ),
        RepositoryStatus::Present { head, changes } => {
            let (branch, ahead, issue) = match head {
                StatusHead::Unavailable => (None, None, Some("Branch status unavailable")),
                StatusHead::Detached => {
                    (Some("Detached HEAD"), None, Some("No upstream configured"))
                }
                StatusHead::Branch { name, upstream } => match upstream {
                    StatusUpstream::Missing => {
                        (Some(name.as_str()), None, Some("No upstream configured"))
                    }
                    StatusUpstream::Tracking { ahead, .. } => {
                        (Some(name.as_str()), Some(ahead.into_inner()), None)
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
    let (status_label, status_color) = match status.class() {
        StatusClass::Pending => ("Changes to review", "text-acc"),
        StatusClass::Clean => ("Up to date", "text-add"),
        StatusClass::Warn | StatusClass::Absent => {
            (issue.unwrap_or("Status unavailable"), "text-warn")
        }
    };
    ProjectStatusPresentation {
        branch,
        local_available,
        ahead,
        issue,
        symbols: symbols.to_string(),
        symbols_description,
        status_label,
        status_color,
    }
}

#[component]
fn ProjectCard(project: ViewerProject, disabled: bool) -> Element {
    let ProjectStatusPresentation {
        branch,
        local_available,
        ahead,
        issue,
        symbols,
        symbols_description,
        status_label,
        status_color,
    } = project_status(&project.status);
    let rendered = project
        .last_rendered_at
        .as_ref()
        .map(|time| format!("Rendered {}", time.display_minute()));
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
                if let Some(branch) = branch {
                    GitBranch { size: 13 }
                    span { class: "truncate", title: branch, "{branch}" }
                } else {
                    NoData {}
                }
                if local_available {
                    span {
                        class: "shrink-0 {status_color}",
                        role: "img",
                        title: symbols_description,
                        aria_label: symbols_description,
                        "[{symbols}]"
                    }
                } else {
                    NoData {}
                }
            }
            p { class: "mt-5 flex items-baseline gap-2 border-y border-line py-3 text-xs text-ink-2",
                span { class: "font-mono text-lg font-medium tabular-nums text-ink",
                    if let Some(count) = ahead {
                        "{count}"
                    } else {
                        NoData {}
                    }
                }
                "Unpushed commits"
            }
            div { class: "mt-5 grid grid-cols-1 gap-2 min-[420px]:grid-cols-2",
                for comparison in [LiveComparison::LocalChanges, LiveComparison::UnpushedCommits] {
                    ProjectComparisonAction {
                        path: project.path.clone(),
                        comparison,
                        available: match comparison {
                            LiveComparison::LocalChanges => local_available,
                            LiveComparison::UnpushedCommits => ahead.is_some(),
                        },
                        disabled,
                        issue,
                    }
                }
            }

            p { class: "mt-4 truncate font-mono text-[10px] text-ink-3",
                if let Some(rendered) = rendered {
                    span { title: rendered.clone(), "{rendered}" }
                } else {
                    NoData {}
                }
            }
        }
    }
}

#[component]
fn ProjectComparisonAction(
    path: RepositoryRoot,
    comparison: LiveComparison,
    available: bool,
    disabled: bool,
    issue: Option<&'static str>,
    #[props(default)] compact: bool,
) -> Element {
    let size = if compact {
        ButtonSize::IconMedium
    } else {
        ButtonSize::Small
    };
    let content = rsx! {
        if comparison == LiveComparison::LocalChanges {
            FileDiff { size: 14 }
        } else {
            ArrowUp { size: 14 }
        }
        if !compact {
            "{comparison.label()}"
        }
    };
    rsx! {
        if available && !disabled {
            Link {
                to: Route::project_diff(&path, comparison),
                draggable: "false",
                class: "{button_classes(ButtonLayout::Inline, ButtonVariant::Outline, size)} min-h-9 select-text max-sm:min-h-11",
                title: comparison.label(),
                aria_label: comparison.label(),
                {content}
            }
        } else {
            Button {
                variant: ButtonVariant::Outline,
                size,
                class: "min-h-9 max-sm:min-h-11",
                state: ButtonState::Disabled,
                title: issue.unwrap_or(comparison.label()),
                aria_label: comparison.label(),
                {content}
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct ProjectsPresentation {
    mode: Memo<ProjectsViewMode>,
    pending: Memo<bool>,
    error: Memo<Option<ViewerClientError>>,
    select: Callback<ProjectsViewMode>,
    retry: Callback<()>,
}

fn use_projects_presentation() -> ProjectsPresentation {
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
fn ProjectsViewToggle(presentation: ProjectsPresentation) -> Element {
    let mode = (presentation.mode)();
    rsx! {
        div {
            class: "flex items-center gap-0.5 rounded-sm border border-line-2 bg-sunk p-0.5",
            role: "group",
            aria_label: "Projects view",
            for (value, label) in [(ProjectsViewMode::Grid, "Grid view"), (ProjectsViewMode::Table, "List view")] {
                Button {
                    variant: if mode == value { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    size: ButtonSize::IconMedium,
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

#[component]
fn ProjectTable(projects: Vec<ViewerProject>, disabled: bool) -> Element {
    use crate::shared::ui::data_table::{DataTable, TableHeading};
    rsx! {
        DataTable {
            caption: "Managed projects",
            header: rsx! {
                TableHeading { "Project" }
                TableHeading { "Branch" }
                TableHeading { "Changes" }
                TableHeading {
                    span { class: "block text-right", "Unpushed" }
                }
                TableHeading { "Last rendered" }
                TableHeading {
                    span { class: "block text-right", "Open diff" }
                }
            },
            for project in projects {
                ProjectTableRow { key: "{project.path}", project, disabled }
            }
        }
    }
}

#[component]
fn ProjectTableRow(project: ViewerProject, disabled: bool) -> Element {
    use crate::shared::ui::data_table::{DataTableRow, TableColumn};
    let status = project_status(&project.status);
    let destination = (!disabled && status.ahead.is_some())
        .then(|| Route::project_diff(&project.path, LiveComparison::UnpushedCommits));
    let rendered = project
        .last_rendered_at
        .as_ref()
        .map(gtl_models::timestamps::MachineTimestamp::display_minute);
    rsx! {
        DataTableRow {
            "data-project-row": "{project.path}",
            aria_label: "{project.name}",
            TableColumn {
                ProjectTableLink { destination: destination.clone(), tabindex: "0",
                    div { class: "min-w-36 max-w-64",
                        span {
                            class: "block truncate font-semibold text-ink",
                            title: "{project.path}",
                            "{project.name}"
                        }
                        span { class: "mt-1 block truncate text-[10px] {status.status_color}",
                            "{status.status_label}"
                        }
                    }
                }
            }
            TableColumn {
                ProjectTableLink { destination: destination.clone(),
                    span { class: "flex max-w-48 items-center gap-1.5 font-mono text-xs",
                        if let Some(branch) = status.branch {
                            GitBranch { size: 13, class: "shrink-0 text-ink-3" }
                            span { class: "truncate select-text", title: branch, "{branch}" }
                        } else {
                            NoData {}
                        }
                    }
                }
            }
            TableColumn {
                ProjectTableLink { destination: destination.clone(),
                    if status.local_available {
                        span {
                            class: "font-mono text-xs {status.status_color}",
                            role: "img",
                            title: status.symbols_description,
                            aria_label: status.symbols_description,
                            "[{status.symbols}]"
                        }
                    } else {
                        NoData {}
                    }
                }
            }
            TableColumn {
                ProjectTableLink { destination: destination.clone(),
                    span { class: "w-full text-right font-mono text-sm tabular-nums text-ink",
                        if let Some(count) = status.ahead {
                            "{count}"
                        } else {
                            NoData {}
                        }
                    }
                }
            }
            TableColumn {
                ProjectTableLink { destination: destination.clone(),
                    span { class: "whitespace-nowrap font-mono text-[10px] text-ink-3",
                        if let Some(rendered) = rendered {
                            "{rendered}"
                        } else {
                            NoData {}
                        }
                    }
                }
            }
            TableColumn {
                div { class: "flex justify-end gap-1",
                    for comparison in [LiveComparison::LocalChanges, LiveComparison::UnpushedCommits] {
                        ProjectComparisonAction {
                            path: project.path.clone(),
                            comparison,
                            available: match comparison {
                                LiveComparison::LocalChanges => status.local_available,
                                LiveComparison::UnpushedCommits => status.ahead.is_some(),
                            },
                            disabled,
                            issue: status.issue,
                            compact: true,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectTableLink(
    destination: Option<Route>,
    #[props(default = "-1")] tabindex: &'static str,
    children: Element,
) -> Element {
    let class = "-mx-4 -my-3 flex h-full min-h-20 items-center px-4 py-3 select-text focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-acc";
    rsx! {
        if let Some(destination) = destination {
            Link {
                to: destination,
                draggable: "false",
                tabindex,
                class,
                {children}
            }
        } else {
            div { class, {children} }
        }
    }
}
