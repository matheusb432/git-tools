use dioxus::prelude::*;
use gtl_models::settings::ProjectsSort;
use gtl_wire::viewer::projects::{ViewerProject, ViewerProjectDiffMode};
use lucide_dioxus::{ArrowUp, Check, FilePenLine, FilePlus, GitBranch, TriangleAlert};

use super::{
    comparison_action::{ProjectActionShape, ProjectComparisonAction},
    comparison_editor::ProjectComparisonEditor,
    presentation::{
        ProjectPresentation, ProjectSignalGlyph, ReviewStatusDot, project_presentation,
    },
    status::ProjectSignal,
};
use crate::{
    app::application_router::Route,
    shared::ui::{
        HoverPopover, HoverPopoverPlacement,
        data_table::{
            DataTable, DataTableActions, DataTableRow, TableColumn, TableHeading,
            TableSortDirection, TableSortHeading,
        },
        no_data::NoData,
        use_hover_popover,
    },
};
const TABLE_LINK_CLASSES: &str = "project-table-link -mx-3 h-11 min-w-0 px-3";

#[component]
pub(super) fn ProjectTable(
    projects: Vec<ViewerProject>,
    disabled: bool,
    sort: ProjectsSort,
    sorting_disabled: bool,
    onsort: EventHandler<ProjectsSort>,
) -> Element {
    use ProjectsSort::{Branch, BranchDescending, Changes, ChangesAscending, Name, NameDescending};
    rsx! {
        DataTable {
            caption: "Managed projects",
            header: rsx! {
                TableHeading { class: "w-8",
                    span { class: "sr-only", "Status" }
                }
                for (label, ascending, descending, initial) in [
                    ("Project", Name, NameDescending, Name),
                    ("Branch", Branch, BranchDescending, Branch),
                    ("Changes", ChangesAscending, Changes, Changes),
                ]
                {
                    {
                        let direction = if sort == ascending {
                            Some(TableSortDirection::Ascending)
                        } else if sort == descending {
                            Some(TableSortDirection::Descending)
                        } else {
                            None
                        };
                        let next = match direction {
                            Some(TableSortDirection::Ascending) => descending,
                            Some(TableSortDirection::Descending) => ascending,
                            None => initial,
                        };
                        rsx! {
                            TableSortHeading {
                                key: "{label}",
                                label,
                                direction,
                                disabled: sorting_disabled,
                                onsort: move |()| onsort(next),
                            }
                        }
                    }
                }
                TableHeading { class: "text-right", "Actions" }
            },
            for project in projects {
                ProjectTableRow { key: "{project.id}", project, disabled }
            }
        }
    }
}

#[component]
fn ProjectTableRow(project: ViewerProject, disabled: bool) -> Element {
    let status = super::loading::use_project_status(&project);
    let load = (status.state)();
    let loading = load.status.is_none() && !load.failed;
    let failed = load.failed;
    let result = load.status.as_ref().map(Ok).or_else(|| {
        failed.then_some(Err(
            crate::shared::viewer_client::ViewerClientError::Internal,
        ))
    });
    let ProjectPresentation {
        branch,
        review,
        local,
        ahead,
        ..
    } = project_presentation(&project, result);
    let destination = (!disabled && local.is_available())
        .then(|| Route::project_diff(&project.path, ViewerProjectDiffMode::Live));
    rsx! {
        DataTableRow {
            "data-testid": "project-table-row",
            "data-project-row": "{project.path}",
            aria_label: "{project.name}",
            aria_busy: loading.to_string(),
            TableColumn { class: "w-8",
                ProjectTableLink {
                    destination: destination.clone(),
                    test_id: "project-table-status",
                    ReviewStatusDot { review, stale: failed && load.status.is_some() }
                }
            }
            TableColumn {
                ProjectTableLink {
                    destination: destination.clone(),
                    tabindex: "0",
                    test_id: "project-table-name",
                    span {
                        class: "block max-w-64 min-w-36 truncate font-semibold text-ink",
                        title: "{project.path}",
                        "{project.name}"
                    }
                }
            }
            TableColumn {
                ProjectTableLink {
                    destination: destination.clone(),
                    test_id: "project-table-branch",
                    span { class: "flex w-48 items-center gap-1.5",
                        if let Some(branch) = branch {
                            GitBranch { size: 13, class: "shrink-0 text-ink-3" }
                            span {
                                class: "truncate select-text",
                                title: branch,
                                "data-testid": "project-table-branch-text",
                                "{branch}"
                            }
                        } else if loading {
                            crate::shared::ui::Skeleton { class: "h-3 w-20" }
                        } else {
                            NoData {}
                        }
                    }
                }
            }
            TableColumn {
                ProjectChanges {
                    project_id: project.id.to_string(),
                    destination,
                    local: local.clone(),
                    ahead: ahead.clone(),
                }
            }
            TableColumn { class: "text-right",
                DataTableActions {
                    span { class: "inline-flex w-8 shrink-0",
                        if ahead.has_changes() {
                            ProjectComparisonAction {
                                path: project.path.clone(),
                                mode: ViewerProjectDiffMode::Snapshot,
                                shape: ProjectActionShape::Icon,
                                disabled,
                            }
                        }
                    }
                    ProjectComparisonAction {
                        path: project.path.clone(),
                        mode: ViewerProjectDiffMode::Live,
                        shape: ProjectActionShape::Icon,
                        disabled: disabled || !local.is_available(),
                    }
                    super::SnapshotHistoryButton { project: project.name.clone() }
                    ProjectComparisonEditor { project: project.clone(), disabled }
                    if failed {
                        crate::shared::ui::Button {
                            variant: crate::shared::ui::ButtonVariant::Ghost,
                            size: crate::shared::ui::ButtonSize::IconSmall,
                            aria_label: "Retry Git status for {project.name}",
                            title: "Retry Git status",
                            onclick: move |_| (status.retry)(()),
                            lucide_dioxus::RefreshCw { size: 14 }
                        }
                    }
                    crate::views::push::PushButton {
                        id: format!("project-push-table-{}", project.id),
                        source: gtl_wire::viewer::push::CreateViewerPush::Project {
                            path: project.path.clone(),
                        },
                        disabled: disabled || failed || !ahead.has_unpushed_commits(),
                        icon_only: true,
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectTableLink(
    destination: Option<Route>,
    test_id: &'static str,
    #[props(default = "-1")] tabindex: &'static str,
    children: Element,
) -> Element {
    let class = TABLE_LINK_CLASSES;
    rsx! {
        if let Some(destination) = destination {
            Link {
                to: destination,
                "data-testid": test_id,
                draggable: "false",
                tabindex,
                class,
                {children}
            }
        } else {
            div { class, tabindex, "data-testid": test_id, {children} }
        }
    }
}

#[component]
fn ProjectChanges(
    project_id: String,
    destination: Option<Route>,
    local: ProjectSignal,
    ahead: ProjectSignal,
) -> Element {
    let id = format!("project-changes-{project_id}");
    let anchor_name = format!("--{id}");
    let hover = use_hover_popover(id.clone());
    let local_description = match &local {
        ProjectSignal::Local { tracked, untracked } if local.has_changes() => {
            format!(
                "{tracked} tracked {} changed; {untracked} untracked {}",
                if tracked.value() == 1 {
                    "file"
                } else {
                    "files"
                },
                if untracked.value() == 1 {
                    "file"
                } else {
                    "files"
                }
            )
        }
        _ => local.description().into_owned(),
    };
    let description = format!("{}; {local_description}", ahead.description());
    let issue = ahead.issue().or_else(|| local.issue());
    let clean = local.is_available()
        && ahead.is_available()
        && !local.has_changes()
        && !ahead.has_changes();
    rsx! {
        div {
            style: "anchor-name: {anchor_name};",
            onmouseenter: move |_| (hover.pointer_enter)(()),
            onmouseleave: move |_| (hover.pointer_leave)(()),
            onfocusin: move |_| (hover.focus_enter)(()),
            onfocusout: move |_| (hover.focus_leave)(()),
            ProjectTableLink {
                destination,
                tabindex: "0",
                test_id: "project-table-changes",
                span {
                    class: "inline-flex min-w-24 items-center gap-2 font-medium tabular-nums text-acc",
                    role: "img",
                    aria_label: description.clone(),
                    if let ProjectSignal::Ahead { count, .. } = &ahead && count.into_inner() > 0 {
                        span {
                            class: "inline-flex items-center gap-0.5",
                            title: ahead.description().into_owned(),
                            ArrowUp { size: 16 }
                            "{count}"
                        }
                    }
                    if let ProjectSignal::Local { tracked, untracked } = &local {
                        if !tracked.is_zero() {
                            span { title: "Tracked changes",
                                FilePenLine { size: 16 }
                            }
                        }
                        if !untracked.is_zero() {
                            span { title: "Untracked files",
                                FilePlus { size: 16 }
                            }
                        }
                    }
                    if clean {
                        span { class: "text-add", title: "No changes pending",
                            Check { size: 17 }
                        }
                    }
                    if let Some(issue) = issue {
                        span { class: "text-warn", title: issue.description(),
                            TriangleAlert { size: 16 }
                        }
                    } else if local == ProjectSignal::Loading || ahead == ProjectSignal::Loading {
                        ProjectSignalGlyph { signal: ProjectSignal::Loading }
                    }
                }
            }
        }
        HoverPopover {
            id,
            anchor_name: anchor_name.clone(),
            aria_label: "Project changes",
            placement: HoverPopoverPlacement::Below,
            span { class: "block whitespace-normal text-xs text-ink-2", "{description}" }
        }
    }
}
