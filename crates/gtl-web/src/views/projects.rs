pub(crate) mod cache;
mod comparison_editor;
mod edit_dialog;
mod import_dialog;
mod loading;
mod pause_toggle;
mod preferences;
mod presentation;
mod status;
mod table;

pub(crate) use comparison_editor::ComparisonBranchEditor;
use dioxus::prelude::*;
use gtl_models::{
    projects::catalogue::ProjectStatusFilter,
    settings::{ProjectsPageSize, ProjectsSort},
};
use gtl_wire::viewer::{
    ViewerHistoryFilter,
    projects::{ViewerProject, ViewerProjectsCursor},
};
use lucide_dioxus::{GitCompareArrows, History, ListFilter, Pencil};

use self::{
    edit_dialog::ProjectEditDialog,
    import_dialog::ImportProjectsDialog,
    loading::{ProjectsActivity, use_projects, use_projects_active},
    preferences::use_projects_presentation,
    table::ProjectTable,
};
use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, LoadingSpinner, PageNotice,
            PanelDialog, ScrollArea, Select, SelectOption,
            dialog::use_dialog_slot,
            pagination::{PageNavigation, PagePosition, Pagination},
            select::SelectVariant,
            use_toast,
        },
    },
};

#[component]
pub(crate) fn ProjectsView(route_active: Memo<bool>) -> Element {
    let language = use_language();
    let mut selection = use_signal(|| {
        None::<(
            ProjectsPageSize,
            ProjectsSort,
            ProjectStatusFilter,
            ViewerProjectsCursor,
        )>
    });
    let mut status_filter = use_signal(ProjectStatusFilter::default);
    let snapshots = use_dialog_slot::<(ViewerHistoryFilter, String)>();
    let importing = use_dialog_slot::<()>();
    let searching = use_dialog_slot::<ViewerProject>();
    let editing = use_dialog_slot::<ViewerProject>();
    let open_commits = use_callback(move |project| searching.open(project));
    use_context_provider(|| OpenCommits(open_commits));
    let open_editor = use_callback(move |project| editing.open(project));
    use_context_provider(|| OpenProjectEditor(open_editor));
    let active = use_projects_active(route_active);
    let presentation = use_projects_presentation(active);
    let status: ReadSignal<ProjectStatusFilter> = status_filter.into();
    let cursor = use_memo(move || {
        selection()
            .filter(|(size, sort, filter, _)| {
                *size == (presentation.page_size)()
                    && *sort == (presentation.sort)()
                    && *filter == status()
            })
            .map_or(ViewerProjectsCursor::First, |(_, _, _, cursor)| cursor)
    });
    let projects = use_projects(
        cursor,
        presentation.page_size,
        presentation.sort,
        status,
        active,
        presentation.ready,
    );
    use_context_provider(|| projects);
    let viewer = use_context::<ViewerContext>();
    let try_again = use_callback(move |()| {
        if (presentation.error)().is_some() {
            (presentation.retry)(());
        } else {
            (projects.refresh)(());
        }
    });
    let sort_projects = use_callback(move |sort| {
        selection.set(None);
        (presentation.select_sort)(sort);
        browser::scroll_element_to_start("projects-content");
    });
    let close_editor = use_callback(move |()| {
        editing.close();
        (projects.refresh)(());
    });
    let filter_projects = use_callback(move |filter| {
        selection.set(None);
        status_filter.set(filter);
        browser::scroll_element_to_start("projects-content");
    });
    use_effect(move || {
        if !route_active() {
            snapshots.close();
            searching.close();
        }
    });
    let page = projects.page.read();
    let result = page
        .as_ref()
        .filter(|load| load.instance_id == viewer.server_instance_id())
        .map(|load| &load.result);
    let items = result.and_then(|result| result.as_ref().ok());
    let error = result.and_then(|result| result.as_ref().err()).cloned();
    let loading_page = (projects.loading_page)();
    let disabled = !viewer.actions_enabled();
    let page_size = (presentation.page_size)();
    let sort = (presentation.sort)();
    let status = status();
    let total = items.map_or(0, |page| page.total() as usize);
    let position = PagePosition::new(
        items.map_or(1, |page| {
            page.count_before() as usize / page_size.into_inner() as usize + 1
        }),
        total.div_ceil(page_size.into_inner() as usize),
    );
    let first = items
        .and_then(|page| page.projects().first())
        .map(|project| project.id.clone());
    let last = items
        .and_then(|page| page.projects().last())
        .map(|project| project.id.clone());

    rsx! {
        if active() {
            ProjectsActivity {}
        }
        main {
            class: "projects-shell h-full min-h-0",
            "data-testid": "projects-view",
            header { class: "projects-header h-14 gap-4 px-4 sm:px-6",
                h1 {
                    id: "projects-heading",
                    tabindex: "-1",
                    class: "projects-title text-xl font-semibold tracking-tight",
                    {t!(language, "navigation-projects")}
                }
                div { class: "projects-header-actions ml-auto gap-3",
                    Button {
                        id: "project-import-trigger",
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Small,
                        state: if disabled { crate::shared::ui::ButtonState::Disabled } else { crate::shared::ui::ButtonState::Enabled },
                        onclick: move |_| importing.open(()),
                        {t!(language, "projects-add")}
                    }
                    ProjectDiffAllButton { disabled }
                    AllSnapshotsButton { onopen: move |trigger| snapshots.open((ViewerHistoryFilter::All, trigger)) }
                }
            }
            if let Some(project) = editing.subject() {
                ProjectEditDialog {
                    key: "{project.id}",
                    project: project.clone(),
                    open: editing.is_open() && route_active(),
                    onclose: close_editor,
                    onclosed: move |()| editing.release(),
                    onsnapshots: move |()| {
                        editing.close();
                        snapshots
                            .open((
                                ViewerHistoryFilter::Project {
                                    name: project.name.clone(),
                                },
                                project_edit_trigger_id(&project),
                            ));
                    },
                }
            }
            if let Some((filter, trigger)) = snapshots.subject() {
                PanelDialog {
                    id: "project-snapshots-dialog",
                    trigger_id: trigger,
                    title: t!(language, "projects-snapshots-title"),
                    variant: crate::shared::ui::panel_dialog::PanelDialogVariant::Table,
                    open: snapshots.is_open(),
                    onclose: move |()| snapshots.close(),
                    onclosed: move |()| snapshots.release(),
                    crate::views::SnapshotHistory { initial_filter: filter }
                }
            }
            if let Some(project) = searching.subject() {
                PanelDialog {
                    id: "project-commit-search-dialog",
                    trigger_id: format!("project-commit-search-{}", project.id),
                    title: t!(language, "commit-search-project-title", project = project.name.to_string()),
                    open: searching.is_open(),
                    onclose: move |()| searching.close(),
                    onclosed: move |()| searching.release(),
                    crate::views::commit_search::CommitFinder { path: project.path }
                }
            }
            if importing.subject().is_some() {
                PanelDialog {
                    id: "project-import-dialog",
                    trigger_id: "project-import-trigger",
                    title: t!(language, "projects-add"),
                    variant: crate::shared::ui::panel_dialog::PanelDialogVariant::Table,
                    open: importing.is_open(),
                    onclose: move |()| importing.close(),
                    onclosed: move |()| importing.release(),
                    ImportProjectsDialog {}
                }
            }
            ScrollArea {
                class: "projects-content min-h-0 p-4 sm:p-6",
                "data-testid": "projects-content",
                id: "projects-content",
                div { class: "mx-auto max-w-7xl",
                    div { class: "projects-toolbar mb-3 gap-3",
                        ProjectStatusFilterSelect { status, onchange: filter_projects }
                        if items.is_some() {
                            p { class: "projects-count",
                                {t!(language, "projects-count", count = total)}
                            }
                        }
                    }
                    if let Some(error) = error.clone().or((presentation.error)()) {
                        div {
                            class: "projects-error mb-4 gap-3 px-3 py-2",
                            role: "alert",
                            span { class: "min-w-0", "{client_error_message(&error, language)} " }
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Small,
                                class: "ml-auto text-warn hover:text-ink active:text-ink",
                                onclick: move |_| try_again(()),
                                {t!(language, "action-try-again")}
                            }
                        }
                    }
                    match items.map(gtl_wire::viewer::projects::ViewerProjectPage::projects) {
                        None if error.is_some() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: t!(language, "projects-unavailable"),
                                message: t!(language, "projects-unavailable-message"),
                            }
                        },
                        None => rsx! {
                            div {
                                class: "flex min-h-64 items-center justify-center",
                                role: "status",
                                aria_label: t!(language, "projects-loading"),
                                LoadingSpinner {}
                            }
                        },
                        Some([]) => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: match status {
                                    ProjectStatusFilter::Active => t!(language, "projects-empty-active"),
                                    ProjectStatusFilter::Paused => t!(language, "projects-empty-paused"),
                                    ProjectStatusFilter::All => t!(language, "projects-empty"),
                                },
                                message: match status {
                                    ProjectStatusFilter::Active => t!(language, "projects-empty-active-message"),
                                    ProjectStatusFilter::Paused => t!(language, "projects-empty-paused-message"),
                                    ProjectStatusFilter::All => t!(language, "projects-empty-message"),
                                },
                            }
                        },
                        Some(items) => rsx! {
                            ProjectTable {
                                projects: items.to_vec(),
                                disabled,
                                sort,
                                sorting_disabled: (presentation.pending)() || loading_page,
                                onsort: sort_projects,
                            }
                        },
                    }
                }
            }
            if total > 0 {
                Pagination {
                    position,
                    label: t!(language, "navigation-projects"),
                    disabled: loading_page || disabled,
                    onselect: move |navigation| {
                        let cursor = match navigation {
                            PageNavigation::First => ViewerProjectsCursor::First,
                            PageNavigation::Previous => {
                                match first.clone() {
                                    Some(id) => ViewerProjectsCursor::Before(id),
                                    None => return,
                                }
                            }
                            PageNavigation::Next => {
                                match last.clone() {
                                    Some(id) => ViewerProjectsCursor::After(id),
                                    None => return,
                                }
                            }
                            PageNavigation::Last => ViewerProjectsCursor::Last,
                        };
                        selection.set(Some((page_size, sort, status, cursor)));
                        browser::scroll_element_to_start("projects-content");
                    },
                    div { class: "flex items-center gap-2",
                        label { class: "shrink-0", r#for: "projects-page-size",
                            {t!(language, "projects-per-page")}
                        }
                        div { class: "w-20",
                            Select {
                                id: "projects-page-size",
                                aria_label: t!(language, "projects-per-page-label"),
                                variant: SelectVariant::Toolbar,
                                value: page_size.to_string(),
                                options: [10, 15, 30]
                                    .map(|size| SelectOption::new(size.to_string(), size.to_string()))
                                    .to_vec(),
                                disabled: (presentation.pending)(),
                                onchange: move |value: String| {
                                    if let Some(size) = value
                                        .parse::<u32>()
                                        .ok()
                                        .and_then(|size| ProjectsPageSize::try_new(size).ok())
                                    {
                                        selection.set(None);
                                        (presentation.select_page_size)(size);
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectStatusFilterSelect(
    status: ProjectStatusFilter,
    onchange: EventHandler<ProjectStatusFilter>,
) -> Element {
    let language = use_language();
    let options = [
        ProjectStatusFilter::Active,
        ProjectStatusFilter::Paused,
        ProjectStatusFilter::All,
    ];
    let value = |filter| match filter {
        ProjectStatusFilter::Active => "active",
        ProjectStatusFilter::Paused => "paused",
        ProjectStatusFilter::All => "all",
    };
    rsx! {
        div { class: "w-40",
            Select {
                id: "projects-status-filter",
                aria_label: t!(language, "projects-status-filter-label"),
                variant: SelectVariant::Toolbar,
                icon: rsx! {
                    ListFilter { size: 15 }
                },
                value: value(status),
                options: options
                    .map(|filter| SelectOption::new(
                        value(filter),
                        match filter {
                            ProjectStatusFilter::Active => t!(language, "projects-status-active"),
                            ProjectStatusFilter::Paused => t!(language, "projects-status-paused"),
                            ProjectStatusFilter::All => t!(language, "projects-status-all"),
                        },
                    ))
                    .to_vec(),
                onchange: move |selected: String| {
                    if let Some(filter) = options
                        .into_iter()
                        .find(|filter| value(*filter) == selected)
                    {
                        onchange.call(filter);
                    }
                },
            }
        }
    }
}

/// Generates diff snapshots for every managed project with commits ahead of its comparison.
#[component]
fn ProjectDiffAllButton(disabled: bool) -> Element {
    let language = use_language();
    let toast = use_toast();
    let mut generate = use_action(move |()| async move {
        match viewer_server::open_unpushed_project_diffs().await {
            Ok(result) => {
                toast.ok(if result.opened_count == 0 {
                    t!(language, "projects-diff-all-empty")
                } else {
                    t!(
                        language,
                        "projects-diff-all-opened",
                        count = result.opened_count
                    )
                });
                if !result.warnings.is_empty() {
                    toast.warn(t!(
                        language,
                        "projects-diff-all-warning",
                        count = result.warnings.len(),
                        projects = result.warnings.join(", ")
                    ));
                }
            }
            Err(error) => toast.client_error(&error),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let pending = generate.pending();
    let label = t!(language, "projects-diff-all");
    rsx! {
        Button {
            id: "project-diff-all-trigger",
            variant: ButtonVariant::Outline,
            size: ButtonSize::Small,
            state: if disabled { ButtonState::Disabled } else if pending { ButtonState::Loading } else { ButtonState::Enabled },
            title: label.clone(),
            onclick: move |_| generate.call(()),
            icon: rsx! {
                GitCompareArrows { size: 15 }
            },
            {label}
        }
    }
}

#[component]
fn AllSnapshotsButton(onopen: EventHandler<String>) -> Element {
    let language = use_language();
    let id = "all-snapshots";
    let label = t!(language, "projects-all-snapshots");
    rsx! {
        Button {
            "data-testid": gtl_web_contracts::test_ids::VIEWER_HISTORY_OPEN.value(),
            id,
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: label.clone(),
            title: label,
            aria_haspopup: "dialog",
            onclick: move |_| onopen.call(id.to_owned()),
            History { size: 15 }
        }
    }
}

#[derive(Clone, Copy)]
struct OpenCommits(Callback<ViewerProject>);

#[derive(Clone, Copy)]
struct OpenProjectEditor(Callback<ViewerProject>);

fn project_edit_trigger_id(project: &ViewerProject) -> String {
    format!("project-edit-{}", project.id)
}

#[component]
fn ProjectEditButton(project: ViewerProject, disabled: bool) -> Element {
    let language = use_language();
    let open = use_context::<OpenProjectEditor>().0;
    let label = t!(
        language,
        "projects-edit",
        project = project.name.to_string()
    );
    rsx! {
        Button {
            id: project_edit_trigger_id(&project),
            variant: ButtonVariant::Ghost,
            size: ButtonSize::IconSmall,
            state: if disabled { ButtonState::Disabled } else { ButtonState::Enabled },
            aria_label: label.clone(),
            aria_haspopup: "dialog",
            title: t!(language, "projects-edit-short"),
            onclick: move |_| open.call(project.clone()),
            Pencil { size: 15 }
        }
    }
}

#[component]
fn CommitSearchButton(project: ViewerProject, disabled: bool) -> Element {
    let language = use_language();
    let open = use_context::<OpenCommits>().0;
    rsx! {
        Button {
            id: format!("project-commit-search-{}", project.id),
            variant: ButtonVariant::Ghost,
            size: ButtonSize::IconSmall,
            state: if disabled { ButtonState::Disabled } else { ButtonState::Enabled },
            aria_label: t!(language, "commit-search-label"),
            title: t!(language, "commit-search-label"),
            onclick: move |_| open.call(project.clone()),
            lucide_dioxus::Search { size: 15 }
        }
    }
}
