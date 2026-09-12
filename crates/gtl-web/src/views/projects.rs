mod card;
mod comparison_action;
mod comparison_editor;
mod loading;
mod motion;
mod presentation;
mod status;
mod table;
mod view_mode;

use dioxus::prelude::*;
use gtl_models::settings::{ProjectsPageSize, ProjectsViewMode};
use gtl_wire::viewer::ViewerHistoryFilter;
use lucide_dioxus::{History, RefreshCw};

use self::{
    card::ProjectCard,
    loading::use_projects,
    table::ProjectTable,
    view_mode::{ProjectsViewToggle, use_projects_presentation},
};
use crate::{
    app::application_layout::ViewerContext,
    shared::{
        browser,
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, PageNotice, PanelDialog, ScrollArea,
            Select, SelectOption, Skeleton,
            pagination::{PagePosition, Pagination},
            select::SelectVariant,
        },
    },
};

const PROJECT_GRID_CLASSES: &str = "projects-grid gap-4";

#[component]
pub(crate) fn ProjectsView() -> Element {
    let mut page_number = use_signal(|| 1);
    let mut snapshots = use_signal(|| None::<(ViewerHistoryFilter, String)>);
    let open_snapshots = use_callback(move |selection| snapshots.set(Some(selection)));
    use_context_provider(|| OpenSnapshots(open_snapshots));
    let projects = use_projects();
    use_context_provider(|| projects);
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
    let mode = (presentation.mode)();
    let disabled = !viewer.actions_enabled();
    let page_size = (presentation.page_size)();
    let total = load.projects.as_ref().map_or(0, Vec::len);
    let position = PagePosition::new(
        page_number(),
        total.div_ceil(page_size.into_inner() as usize),
    );
    let page_start = (position.number() - 1) * page_size.into_inner() as usize;

    rsx! {
        document::Title { "Projects - git-tools" }
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
                    SnapshotHistoryButton {}
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
            ScrollArea {
                class: "projects-content min-h-0 p-4 sm:p-6",
                "data-testid": "projects-content",
                id: "projects-content",
                div { class: "mx-auto max-w-7xl",
                    if let Some(error) = load.error.or((presentation.error)()) {
                        div {
                            class: "projects-error mb-4 gap-3 px-3 py-2",
                            role: "alert",
                            span { class: "min-w-0",
                                "{error.message()} "
                                if load.projects.is_some() {
                                    "Showing the last known status."
                                }
                            }
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Small,
                                class: "ml-auto text-warn hover:text-ink active:text-ink",
                                onclick: move |_| try_again(()),
                                "Try again"
                            }
                        }
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
                            div { class: PROJECT_GRID_CLASSES, aria_label: "Loading projects",
                                for index in 0..6 {
                                    Skeleton { key: "{index}", class: "h-52 rounded-panel" }
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
                            ProjectTable {
                                projects: items
                                    .iter()
                                    .skip(page_start)
                                    .take(page_size.into_inner() as usize)
                                    .cloned()
                                    .collect(),
                                disabled,
                            }
                        },
                        Some(items) => rsx! {
                            div { class: PROJECT_GRID_CLASSES, aria_label: "Managed projects",
                                for project in items.iter().skip(page_start).take(page_size.into_inner() as usize) {
                                    ProjectCard { key: "{project.path}", project: project.clone(), disabled }
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
                    onselect: move |navigation| {
                        page_number.set(position.select(navigation));
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
                                        page_number.set(1);
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
