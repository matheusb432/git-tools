use dioxus::prelude::*;
use gtl_models::live_views::LiveComparison;
use gtl_wire::viewer::projects::ViewerProject;
use lucide_dioxus::GitBranch;

use super::{
    comparison_action::{ProjectActionShape, ProjectComparisonAction},
    comparison_editor::ProjectComparisonEditor,
    presentation::{
        ProjectPresentation, ProjectSignalGlyph, ReviewStatusDot, project_presentation,
    },
};
use crate::{
    app::application_router::Route,
    shared::ui::{
        data_table::{DataTable, DataTableRow, TableColumn, TableHeading},
        no_data::NoData,
    },
};
const TABLE_LINK_CLASSES: &str = "project-table-link -mx-3 h-11 min-w-0 px-3";

#[component]
pub(super) fn ProjectTable(projects: Vec<ViewerProject>, disabled: bool) -> Element {
    rsx! {
        DataTable {
            caption: "Managed projects",
            header: rsx! {
                TableHeading { class: "w-8",
                    span { class: "sr-only", "Status" }
                }
                TableHeading { "Project" }
                TableHeading { "Branch" }
                TableHeading { "Changes" }
                TableHeading { class: "text-right", "Ahead" }
                TableHeading { "Last rendered" }
                TableHeading { class: "text-right", "Open diff" }
            },
            for project in projects {
                ProjectTableRow { key: "{project.path}", project, disabled }
            }
        }
    }
}

#[component]
fn ProjectTableRow(project: ViewerProject, disabled: bool) -> Element {
    let ProjectPresentation {
        branch,
        review,
        local,
        ahead,
        ahead_label,
        rendered,
        ..
    } = project_presentation(&project);
    let destination = (!disabled && ahead.is_available())
        .then(|| Route::project_diff(&project.path, LiveComparison::UnpushedCommits));
    rsx! {
        DataTableRow {
            "data-testid": "project-table-row",
            "data-project-row": "{project.path}",
            aria_label: "{project.name}",
            TableColumn { class: "w-8",
                ProjectTableLink {
                    destination: destination.clone(),
                    test_id: "project-table-status",
                    ReviewStatusDot { review }
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
                    span { class: "flex max-w-48 items-center gap-1.5",
                        if let Some(branch) = branch {
                            GitBranch { size: 13, class: "shrink-0 text-ink-3" }
                            span {
                                class: "truncate select-text",
                                title: branch,
                                "data-testid": "project-table-branch-text",
                                "{branch}"
                            }
                        } else {
                            NoData {}
                        }
                    }
                }
            }
            TableColumn {
                ProjectTableLink {
                    destination: destination.clone(),
                    test_id: "project-table-changes",
                    ProjectSignalGlyph { signal: local.clone() }
                }
            }
            TableColumn { class: "text-right",
                ProjectTableLink {
                    destination: destination.clone(),
                    test_id: "project-table-unpushed",
                    align_end: true,
                    ProjectSignalGlyph { signal: ahead.clone() }
                }
            }
            TableColumn {
                ProjectTableLink { destination, test_id: "project-table-rendered",
                    span { class: "text-xs text-ink-3",
                        if let Some(rendered) = rendered {
                            "{rendered}"
                        } else {
                            NoData {}
                        }
                    }
                }
            }
            TableColumn { class: "text-right",
                div { class: "flex justify-end gap-1",
                    ProjectComparisonAction {
                        path: project.path.clone(),
                        comparison: LiveComparison::LocalChanges,
                        signal: local,
                        label: LiveComparison::LocalChanges.label(),
                        shape: ProjectActionShape::Icon,
                        disabled,
                    }
                    ProjectComparisonAction {
                        path: project.path.clone(),
                        comparison: LiveComparison::UnpushedCommits,
                        signal: ahead,
                        label: ahead_label,
                        shape: ProjectActionShape::Icon,
                        disabled,
                    }
                    ProjectComparisonEditor { project: project.clone(), disabled }
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
    #[props(default)] align_end: bool,
    children: Element,
) -> Element {
    let class = if align_end {
        format!("{TABLE_LINK_CLASSES} justify-end")
    } else {
        TABLE_LINK_CLASSES.to_owned()
    };
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
            div { class, "data-testid": test_id, {children} }
        }
    }
}
