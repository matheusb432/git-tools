pub(crate) mod cache;
mod comparison_action;
mod comparison_editor;
mod import_dialog;
mod loading;
mod preferences;
mod presentation;
mod status;
mod table;

pub(crate) use comparison_editor::{ComparisonBranchEditor, ComparisonEditorTrigger};
use dioxus::prelude::*;
use gtl_models::settings::{ProjectsPageSize, ProjectsSort};
use gtl_wire::viewer::{ViewerHistoryFilter, projects::ViewerProjectsCursor};
use lucide_dioxus::{GitCompareArrows, History};

use self::{
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
            pagination::{PageNavigation, PagePosition, Pagination},
            select::SelectVariant,
            use_toast,
        },
    },
};

#[component]
pub(crate) fn ProjectsView(route_active: Memo<bool>) -> Element {
    let language = use_language();
    let mut selection =
        use_signal(|| None::<(ProjectsPageSize, ProjectsSort, ViewerProjectsCursor)>);
    let mut snapshots = use_signal(|| None::<(ViewerHistoryFilter, String)>);
    let mut importing = use_signal(|| false);
    let mut searching = use_signal(|| None::<gtl_wire::viewer::projects::ViewerProject>);
    let open_commits = use_callback(move |project| searching.set(Some(project)));
    use_context_provider(|| OpenCommits(open_commits));
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
        if !route_active() && (snapshots.peek().is_some() || searching.peek().is_some()) {
            snapshots.set(None);
            searching.set(None);
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
                        onclick: move |_| importing.set(true),
                        {t!(language, "projects-add")}
                    }
                    ProjectDiffAllButton { disabled }
                    SnapshotHistoryButton {}
                }
            }
            if let Some((filter, trigger)) = snapshots() {
                PanelDialog {
                    id: "project-snapshots-dialog",
                    trigger_id: trigger,
                    title: t!(language, "projects-snapshots-title"),
                    variant: crate::shared::ui::panel_dialog::PanelDialogVariant::Table,
                    open: true,
                    onclose: move |()| snapshots.set(None),
                    crate::views::SnapshotHistory { initial_filter: filter }
                }
            }
            if let Some(project) = searching() {
                PanelDialog {
                    id: "project-commit-search-dialog",
                    trigger_id: format!("project-commit-search-{}", project.id),
                    title: t!(language, "commit-search-project-title", project = project.name.to_string()),
                    open: true,
                    onclose: move |()| searching.set(None),
                    crate::views::commit_search::CommitFinder { path: project.path }
                }
            }
            if importing() {
                PanelDialog {
                    id: "project-import-dialog",
                    trigger_id: "project-import-trigger",
                    title: t!(language, "projects-add"),
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
                                title: t!(language, "projects-empty"),
                                message: t!(language, "projects-empty-message"),
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
                        selection.set(Some((page_size, sort, cursor)));
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
fn SnapshotHistoryButton(project: Option<gtl_models::paths::ProjectName>) -> Element {
    let language = use_language();
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
        || t!(language, "projects-all-snapshots"),
        |name| {
            t!(
                language,
                "projects-snapshots-for",
                project = name.to_string()
            )
        },
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

mod push_confirmation_setting;

#[derive(Clone, Copy)]
struct OpenCommits(Callback<gtl_wire::viewer::projects::ViewerProject>);

#[component]
fn CommitSearchButton(
    project: gtl_wire::viewer::projects::ViewerProject,
    disabled: bool,
) -> Element {
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
