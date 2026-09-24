pub(crate) mod cache;
mod card;
mod comparison_action;
mod comparison_editor;
mod import_dialog;
mod loading;
mod presentation;
mod status;
mod table;
mod view_mode;

pub(crate) use comparison_editor::{ComparisonBranchEditor, ComparisonEditorTrigger};
use dioxus::prelude::*;
use gtl_models::settings::{ProjectsPageSize, ProjectsSort, ProjectsViewMode};
use gtl_wire::viewer::{ViewerHistoryFilter, projects::ViewerProjectsCursor};
use lucide_dioxus::History;

use self::{
    card::ProjectCard,
    import_dialog::ImportProjectsDialog,
    loading::{ProjectsActivity, use_projects, use_projects_active},
    table::ProjectTable,
    view_mode::{ProjectsViewToggle, use_projects_presentation},
};
use crate::{
    app::application_layout::ViewerContext,
    shared::{
        browser,
        ui::{
            Button, ButtonSize, ButtonVariant, PageNotice, PanelDialog, ScrollArea, Select,
            SelectOption, Skeleton,
            pagination::{PageNavigation, PagePosition, Pagination},
            select::SelectVariant,
        },
    },
};

const PROJECT_GRID_CLASSES: &str = "projects-grid gap-4";

#[component]
pub(crate) fn ProjectsView(route_active: Memo<bool>) -> Element {
    let mut selection =
        use_signal(|| None::<(ProjectsPageSize, ProjectsSort, ViewerProjectsCursor)>);
    let mut snapshots = use_signal(|| None::<(ViewerHistoryFilter, String)>);
    let mut importing = use_signal(|| false);
    let open_snapshots = use_callback(move |selection| snapshots.set(Some(selection)));
    use_context_provider(|| OpenSnapshots(open_snapshots));
    let active = use_projects_active(route_active);
    let presentation = use_projects_presentation(active);
    let cursor = use_memo(move || {
        selection()
            .filter(|(size, sort, _)| {
                *size == (presentation.page_size)() && *sort == (presentation.sort)()
            })
            .map_or(ViewerProjectsCursor::First, |(_, _, cursor)| cursor)
    });
    let projects = use_projects(
        cursor,
        presentation.page_size,
        presentation.sort,
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
    use_effect(move || {
        if !route_active() && snapshots.peek().is_some() {
            snapshots.set(None);
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
    let mode = (presentation.mode)();
    let disabled = !viewer.actions_enabled();
    let page_size = (presentation.page_size)();
    let sort = (presentation.sort)();
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
                    "Projects"
                }
                div { class: "projects-header-actions ml-auto gap-3",
                    Button {
                        id: "project-import-trigger",
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Small,
                        state: if disabled { crate::shared::ui::ButtonState::Disabled } else { crate::shared::ui::ButtonState::Enabled },
                        onclick: move |_| importing.set(true),
                        "Add projects"
                    }
                    SnapshotHistoryButton {}
                    ProjectsViewToggle { presentation }
                }
            }
            if let Some((filter, trigger)) = snapshots() {
                PanelDialog {
                    id: "project-snapshots-dialog",
                    trigger_id: trigger,
                    title: "Snapshots",
                    variant: crate::shared::ui::panel_dialog::PanelDialogVariant::Table,
                    open: true,
                    onclose: move |()| snapshots.set(None),
                    crate::views::SnapshotHistory { initial_filter: filter }
                }
            }
            if importing() {
                PanelDialog {
                    id: "project-import-dialog",
                    trigger_id: "project-import-trigger",
                    title: "Add projects",
                    variant: crate::shared::ui::panel_dialog::PanelDialogVariant::Table,
                    open: true,
                    onclose: move |()| importing.set(false),
                    ImportProjectsDialog {}
                }
            }
            ScrollArea {
                class: "projects-content min-h-0 p-4 sm:p-6",
                "data-testid": "projects-content",
                id: "projects-content",
                div { class: "mx-auto max-w-7xl",
                    if let Some(error) = error.clone().or((presentation.error)()) {
                        div {
                            class: "projects-error mb-4 gap-3 px-3 py-2",
                            role: "alert",
                            span { class: "min-w-0", "{error} " }
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Small,
                                class: "ml-auto text-warn hover:text-ink active:text-ink",
                                onclick: move |_| try_again(()),
                                "Try again"
                            }
                        }
                    }
                    match items.map(gtl_wire::viewer::projects::ViewerProjectPage::projects) {
                        None if error.is_some() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: "Projects unavailable",
                                message: "Check the project catalogue and try again.",
                            }
                        },
                        None => rsx! {
                            div { class: PROJECT_GRID_CLASSES, aria_label: "Loading projects",
                                for index in 0..6 {
                                    Skeleton { key: "{index}", class: "h-52 rounded-panel" }
                                }
                            }
                        },
                        Some([]) => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: "No managed projects",
                                message: "Projects managed in Git Tools appear here.",
                            }
                        },
                        Some(items) if mode == ProjectsViewMode::Table => rsx! {
                            ProjectTable {
                                projects: items.to_vec(),
                                disabled,
                                sort,
                                sorting_disabled: (presentation.pending)() || loading_page,
                                onsort: sort_projects,
                            }
                        },
                        Some(items) => rsx! {
                            div { class: PROJECT_GRID_CLASSES, aria_label: "Managed projects",
                                for project in items.iter() {
                                    ProjectCard { key: "{project.id}", project: project.clone(), disabled }
                                }
                            }
                        },
                    }
                }
            }
            if total > 0 {
                Pagination {
                    position,
                    label: "Projects",
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
                        selection.set(Some((page_size, sort, cursor)));
                        browser::scroll_element_to_start("projects-content");
                    },
                    div { class: "flex items-center gap-2",
                        label { class: "shrink-0", r#for: "projects-page-size", "Per page" }
                        div { class: "w-20",
                            Select {
                                id: "projects-page-size",
                                aria_label: "Projects per page",
                                variant: SelectVariant::Toolbar,
                                value: page_size.to_string(),
                                options: [10, 15, 30]
                                    .map(|size| SelectOption::new(size.to_string(), size.to_string()))
                                    .to_vec(),
                                disabled: (presentation.pending)(),
                                onchange: move |event: FormEvent| {
                                    if let Some(size) = event
                                        .value()
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

#[derive(Clone, Copy)]
struct OpenSnapshots(Callback<(ViewerHistoryFilter, String)>);

#[component]
fn SnapshotHistoryButton(project: Option<gtl_models::paths::ProjectName>) -> Element {
    let open = use_context::<OpenSnapshots>();
    let id = project.as_ref().map_or_else(
        || "all-snapshots".to_owned(),
        |name| {
            format!(
                "project-snapshots-{}",
                name.as_str().bytes().fold(String::new(), |mut text, byte| {
                    use std::fmt::Write as _;
                    let _ = write!(text, "{byte:02x}");
                    text
                })
            )
        },
    );
    let label = project.as_ref().map_or_else(
        || "All snapshots".to_owned(),
        |name| format!("Snapshots for {name}"),
    );
    let filter = project.map_or(ViewerHistoryFilter::All, |name| {
        ViewerHistoryFilter::Project { name }
    });
    let trigger = id.clone();
    rsx! {
        Button {
            "data-testid": (filter == ViewerHistoryFilter::All)
                .then_some(gtl_web_contracts::test_ids::VIEWER_HISTORY_OPEN.value()),
            id,
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: label.clone(),
            title: label,
            aria_haspopup: "dialog",
            onclick: move |_| open.0.call((filter.clone(), trigger.clone())),
            History { size: 15 }
        }
    }
}
