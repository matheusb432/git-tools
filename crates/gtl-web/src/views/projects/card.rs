use dioxus::prelude::*;
use gtl_models::live_views::LiveComparison;
use gtl_wire::viewer::projects::ViewerProject;
use lucide_dioxus::{GitBranch, GitCompare};

use super::{
    comparison_action::{ProjectActionShape, ProjectComparisonAction},
    comparison_editor::ProjectComparisonEditor,
    presentation::{ProjectPresentation, ReviewStatusDot, project_presentation},
};
use crate::shared::ui::no_data::NoData;

#[component]
pub(super) fn ProjectCard(project: ViewerProject, disabled: bool) -> Element {
    let ProjectPresentation {
        branch,
        comparison_base,
        review,
        local,
        ahead,
        ahead_label,
        issue,
        rendered,
    } = project_presentation(&project);
    rsx! {
        article {
            class: "project-card min-w-0 gap-3 p-4",
            "data-project-card": "{project.path}",
            aria_label: "{project.name}",
            div { class: "project-card-header h-8 min-w-0 gap-2.5",
                ReviewStatusDot { review }
                h2 {
                    class: "project-card-title min-w-0 text-base font-semibold tracking-tight",
                    title: "{project.path}",
                    "{project.name}"
                }
                div { class: "ml-auto",
                    ProjectComparisonEditor { project: project.clone(), disabled }
                }
            }
            p { class: "project-card-meta min-w-0 gap-1.5 text-xs",
                if let Some(branch) = branch {
                    GitBranch { size: 13, class: "shrink-0 text-ink-3" }
                    span { class: "truncate", title: branch, "{branch}" }
                } else {
                    NoData {}
                }
                if let Some(base) = comparison_base {
                    GitCompare { size: 13, class: "ml-2 shrink-0 text-ink-3" }
                    span { class: "truncate", title: "Comparison branch", "{base}" }
                }
            }
            div { class: "project-card-actions",
                ProjectComparisonAction {
                    path: project.path.clone(),
                    comparison: LiveComparison::LocalChanges,
                    signal: local,
                    label: LiveComparison::LocalChanges.label(),
                    shape: ProjectActionShape::SignalRow,
                    disabled,
                }
                ProjectComparisonAction {
                    path: project.path.clone(),
                    comparison: LiveComparison::UnpushedCommits,
                    signal: ahead,
                    label: ahead_label,
                    shape: ProjectActionShape::SignalRow,
                    disabled,
                }
            }
            if let Some(issue) = issue {
                p { class: "text-xs break-words text-warn", "{issue}" }
            }
            p { class: "truncate text-xs text-ink-3",
                if let Some(rendered) = rendered {
                    span { title: "Last rendered", "Rendered {rendered}" }
                } else {
                    "Never rendered"
                }
            }
        }
    }
}
