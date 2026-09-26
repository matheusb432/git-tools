use dioxus::prelude::*;
use gtl_wire::viewer::projects::ViewerProject;
use lucide_dioxus::{GitBranch, GitCompare};

use super::{
    comparison_action::{ProjectActionShape, ProjectComparisonAction},
    comparison_editor::{ProjectComparisonEditor, comparison_popover_id},
    presentation::{
        ProjectPresentation, ProjectSignalGlyph, ReviewStatusDot, project_presentation,
    },
};
use crate::shared::{
    date_display::use_date_display,
    i18n::{t, use_language},
    ui::no_data::NoData,
};

#[component]
pub(super) fn ProjectCard(project: ViewerProject, disabled: bool) -> Element {
    let language = use_language();
    let date_display = use_date_display();
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
        comparison_base,
        review,
        local,
        ahead,
        ahead_label,
        issue,
        comparison_branch_fix,
    } = project_presentation(&project, result, language);
    let rendered = project.last_rendered_at.as_ref().map(|rendered_at| {
        let date = date_display.show(rendered_at, language);
        let style = if date.relative {
            "relative"
        } else {
            "absolute"
        };
        let label = t!(
            language,
            "projects-rendered",
            time = date.text,
            style = style
        );
        (label, date.exact)
    });
    rsx! {
        article {
            class: "project-card min-w-0 gap-3 p-4",
            "data-project-card": "{project.path}",
            aria_label: "{project.name}",
            aria_busy: loading.to_string(),
            div { class: "project-card-header h-8 min-w-0 gap-2.5",
                ReviewStatusDot { review, stale: failed && load.status.is_some() }
                h2 {
                    class: "project-card-title min-w-0 text-base font-semibold tracking-tight",
                    title: "{project.path}",
                    "{project.name}"
                }
                div { class: "ml-auto",
                    crate::views::push::PushButton {
                        id: format!("project-push-card-{}", project.id),
                        source: gtl_wire::viewer::push::CreateViewerPush::Project {
                            path: project.path.clone(),
                        },
                        disabled: disabled || failed || !ahead.has_unpushed_commits(),
                    }
                    super::SnapshotHistoryButton { project: project.name.clone() }
                    ProjectComparisonEditor { project: project.clone(), disabled }
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
                }
            }
            p { class: "project-card-meta h-4 min-w-0 gap-1.5 text-xs",
                if let Some(branch) = branch {
                    GitBranch { size: 13, class: "shrink-0 text-ink-3" }
                    span { class: "truncate", title: branch.to_string(), "{branch}" }
                } else if loading {
                    crate::shared::ui::Skeleton { class: "h-3 w-20" }
                } else {
                    NoData {}
                }
                if let Some(base) = comparison_base {
                    GitCompare { size: 13, class: "ml-2 shrink-0 text-ink-3" }
                    span {
                        class: "truncate",
                        title: t!(language, "projects-comparison-branch"),
                        "{base}"
                    }
                }
            }
            div { class: "flex min-h-4 flex-wrap items-center gap-x-5 gap-y-2 text-xs text-ink-2",
                span { class: "flex items-center gap-2",
                    ProjectSignalGlyph { signal: local.clone() }
                    {t!(language, "comparison-local-changes")}
                }
                span { class: "flex items-center gap-2",
                    ProjectSignalGlyph { signal: ahead.clone() }
                    "{ahead_label}"
                }
            }
            div { class: "flex items-center gap-2 border-t border-line pt-3",
                ProjectComparisonAction {
                    path: project.path.clone(),
                    shape: ProjectActionShape::Labeled,
                    disabled: disabled || !local.is_available(),
                }
            }
            if let Some(issue) = issue {
                div { class: "flex flex-wrap items-center gap-x-3 gap-y-1",
                    p { class: "min-w-0 text-xs break-words text-warn", "{issue}" }
                    if comparison_branch_fix {
                        crate::shared::ui::Button {
                            variant: crate::shared::ui::ButtonVariant::Ghost,
                            size: crate::shared::ui::ButtonSize::Small,
                            popovertarget: comparison_popover_id(&project.path),
                            popovertargetaction: "show",
                            disabled,
                            {t!(language, "projects-change-comparison-branch")}
                        }
                    }
                }
            }
            p { class: "truncate text-xs text-ink-3",
                if let Some((label, exact)) = rendered {
                    span { title: exact, "{label}" }
                } else {
                    {t!(language, "projects-never-rendered")}
                }
            }
        }
    }
}
