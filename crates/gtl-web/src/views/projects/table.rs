use dioxus::prelude::*;
use gtl_models::{projects::catalogue::ProjectStatus, settings::ProjectsSort};
use gtl_wire::viewer::projects::ViewerProject;
use lucide_dioxus::{ArrowUp, Check, CirclePause, FilePenLine, FilePlus, GitBranch, TriangleAlert};

use super::{
    pause_toggle::ProjectPauseToggle,
    presentation::{
        ProjectPresentation, ProjectStatusSpinner, ReviewStatusDot, project_presentation,
    },
    status::ProjectSignal,
};
use crate::{
    app::application_router::Route,
    shared::{
        i18n::{t, use_language},
        ui::{
            HoverPopover, HoverPopoverPlacement,
            data_table::{
                DataTable, DataTableActions, DataTableRow, TableColumn, TableHeading,
                TableSortDirection, TableSortHeading,
            },
            no_data::NoData,
            use_hover_popover,
        },
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
    let language = use_language();
    rsx! {
        DataTable {
            "data-tour": super::tours::PROJECTS_TABLE.value(),
            caption: t!(language, "projects-table-caption"),
            header: rsx! {
                TableHeading { class: "w-8",
                    span { class: "sr-only", {t!(language, "projects-table-status")} }
                }
                for (label, ascending, descending, initial) in [
                    (t!(language, "projects-table-project"), Name, NameDescending, Name),
                    (t!(language, "projects-table-branch"), Branch, BranchDescending, Branch),
                    (t!(language, "projects-table-changes"), ChangesAscending, Changes, Changes),
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
                TableHeading { class: "text-right", {t!(language, "projects-table-actions")} }
            },
            for project in projects {
                ProjectTableRow { key: "{project.id}", project, disabled }
            }
        }
    }
}

#[component]
fn ProjectTableRow(project: ViewerProject, disabled: bool) -> Element {
    let language = use_language();
    let status = super::loading::use_project_status(&project);
    let load = (status.state)();
    let loading = load.status.is_none() && !load.failed;
    let failed = load.failed;
    let result = load.status.as_ref().map(Ok).or_else(|| {
        failed.then_some(Err(
            crate::shared::viewer_client::ViewerClientError::InvalidMessage,
        ))
    });
    let ProjectPresentation {
        branch,
        review,
        local,
        ahead,
    } = project_presentation(result, language);
    let destination =
        (!disabled && local.is_available()).then(|| Route::project_diff(&project.path));
    let paused = project.status == ProjectStatus::Paused;
    rsx! {
        DataTableRow {
            "data-testid": "project-table-row",
            "data-project-row": "{project.path}",
            "data-paused": paused.to_string(),
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
                    span { class: "flex max-w-80 min-w-36 items-center gap-2",
                        span {
                            class: "project-name min-w-0 truncate",
                            title: "{project.path}",
                            "{project.name}"
                        }
                        if paused {
                            span {
                                class: "project-paused-indicator",
                                role: "img",
                                aria_label: t!(language, "projects-paused-status"),
                                title: t!(language, "projects-paused-status"),
                                CirclePause { size: 14 }
                            }
                        }
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
                                title: branch.to_string(),
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
                DataTableActions { "data-tour": super::tours::PROJECTS_ACTIONS.value(),
                    super::CommitSearchButton { project: project.clone(), disabled }
                    ProjectPauseToggle { project: project.clone(), disabled }
                    super::ProjectEditButton { project: project.clone(), disabled }
                    if failed {
                        crate::shared::ui::Button {
                            variant: crate::shared::ui::ButtonVariant::Ghost,
                            size: crate::shared::ui::ButtonSize::IconSmall,
                            aria_label: t!(language, "projects-retry-status-for", project = project.name.to_string()),
                            title: t!(language, "projects-retry-status"),
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
    let language = use_language();
    let id = format!("project-changes-{project_id}");
    let anchor_name = format!("--{id}");
    let hover = use_hover_popover(id.clone());
    let local_description = match &local {
        ProjectSignal::Local { tracked, untracked } if local.has_changes() => t!(
            language,
            "projects-local-counts",
            tracked = tracked.value(),
            untracked = untracked.value()
        ),
        _ => local.description(language),
    };
    let ahead_description = ahead.description(language);
    let description = format!("{ahead_description}; {local_description}");
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
                            title: ahead_description,
                            ArrowUp { size: 16 }
                            "{count}"
                        }
                    }
                    if let ProjectSignal::Local { tracked, untracked } = &local {
                        if !tracked.is_zero() {
                            span { title: t!(language, "projects-tracked-changes"),
                                FilePenLine { size: 16 }
                            }
                        }
                        if !untracked.is_zero() {
                            span { title: t!(language, "projects-local-untracked"),
                                FilePlus { size: 16 }
                            }
                        }
                    }
                    if clean {
                        span {
                            class: "text-add",
                            title: t!(language, "projects-no-changes"),
                            Check { size: 17 }
                        }
                    }
                    if let Some(issue) = issue {
                        span {
                            class: "text-warn",
                            title: issue.description(language),
                            TriangleAlert { size: 16 }
                        }
                    } else if local == ProjectSignal::Loading || ahead == ProjectSignal::Loading {
                        ProjectStatusSpinner {}
                    }
                }
            }
        }
        HoverPopover {
            id,
            anchor_name: anchor_name.clone(),
            aria_label: t!(language, "projects-changes-popover"),
            placement: HoverPopoverPlacement::Below,
            span { class: "block whitespace-normal text-xs text-ink-2", "{description}" }
        }
    }
}
