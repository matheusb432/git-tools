use dioxus::prelude::*;
use gtl_wire::viewer::projects::{ViewerProject, ViewerProjectDiffMode};
use lucide_dioxus::{GitBranch, GitCompare};

use super::{
    comparison_action::{ProjectActionShape, ProjectComparisonAction},
    comparison_editor::ProjectComparisonEditor,
    presentation::{
        ProjectPresentation, ProjectSignalGlyph, ReviewStatusDot, project_presentation,
    },
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
                    super::SnapshotHistoryButton { project: project.name.clone() }
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
            div { class: "flex flex-wrap items-center gap-x-5 gap-y-2 text-xs text-ink-2",
                span { class: "flex items-center gap-2",
                    ProjectSignalGlyph { signal: local.clone() }
                    "Local changes"
                }
                span { class: "flex items-center gap-2",
                    ProjectSignalGlyph { signal: ahead.clone() }
                    "{ahead_label}"
                }
            }
            div { class: "flex items-center gap-2 border-t border-line pt-3",
                ProjectComparisonAction {
                    path: project.path.clone(),
                    mode: ViewerProjectDiffMode::Snapshot,
                    shape: ProjectActionShape::Labeled,
                    disabled: disabled || !ahead.is_available(),
                    unavailable_reason: (!ahead.is_available()).then(|| ahead.description().into_owned()),
                }
                ProjectComparisonAction {
                    path: project.path.clone(),
                    mode: ViewerProjectDiffMode::Live,
                    shape: ProjectActionShape::Labeled,
                    disabled: disabled || !local.is_available(),
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
