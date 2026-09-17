use std::borrow::Cow;

use dioxus::prelude::*;
use gtl_models::{
    repository::status::{RepositoryStatus, StatusChanges, StatusHead},
    timestamps::MachineTimestamp,
};
use gtl_wire::viewer::projects::{
    ViewerProject, ViewerProjectBranchComparison, ViewerProjectStatus,
};

use super::status::{ProjectIssue, ProjectReview, ProjectSignal, ProjectStatus};
use crate::shared::{ui::LoadingSpinner, viewer_client::ViewerClientError};

impl ProjectIssue {
    pub(super) fn description(&self) -> &str {
        match self {
            Self::RequestFailed => "Git status unavailable",
            Self::RepositoryAbsent => "Repository not found",
            Self::HeadUnavailable => "Branch status unavailable",
            Self::WorkingTreeUnavailable => "Working-tree status unavailable",
            Self::UpstreamMissing => "No upstream configured",
            Self::ComparisonUnavailable(reason) => reason,
        }
    }
}

impl ProjectSignal {
    pub(super) fn glyph(&self) -> String {
        match self {
            Self::Local { tracked, untracked } => format!(
                "[{}]",
                StatusChanges::from_counts(*tracked, *untracked).symbols()
            ),
            Self::Ahead { count, .. } => format!("↑{}", count.into_inner()),
            Self::Loading => String::new(),
            Self::Unavailable(_) => "-".to_owned(),
        }
    }

    pub(super) fn text_classes(&self) -> &'static str {
        match self {
            Self::Local { tracked, untracked } if !tracked.is_zero() || !untracked.is_zero() => {
                "text-acc"
            }
            Self::Ahead { count, .. } if count.into_inner() > 0 => "text-acc",
            Self::Loading | Self::Local { .. } | Self::Ahead { .. } => "text-ink-3",
            Self::Unavailable(_) => "text-warn",
        }
    }

    pub(super) fn description(&self) -> Cow<'_, str> {
        match self {
            Self::Local { tracked, untracked } => {
                match (!tracked.is_zero(), !untracked.is_zero()) {
                    (true, true) => "Modified and untracked files".into(),
                    (true, false) => "Modified files".into(),
                    (false, true) => "Untracked files".into(),
                    (false, false) => "Working tree clean".into(),
                }
            }
            Self::Ahead { count, base } => match (count.into_inner(), base) {
                (0, None) => "Nothing to push".into(),
                (1, None) => "1 unpushed commit".into(),
                (count, None) => format!("{count} unpushed commits").into(),
                (0, Some(base)) => format!("Nothing ahead of {base}").into(),
                (1, Some(base)) => format!("1 commit ahead of {base}").into(),
                (count, Some(base)) => format!("{count} commits ahead of {base}").into(),
            },
            Self::Loading => "Loading Git status".into(),
            Self::Unavailable(issue) => issue.description().into(),
        }
    }
}

pub(super) struct ProjectPresentation<'a> {
    pub(super) branch: Option<&'a str>,
    pub(super) comparison_base: Option<&'a str>,
    pub(super) review: ProjectReview,
    pub(super) local: ProjectSignal,
    pub(super) ahead: ProjectSignal,
    pub(super) ahead_label: &'static str,
    pub(super) issue: Option<String>,
    pub(super) rendered: Option<String>,
}

pub(super) fn project_presentation<'a>(
    project: &ViewerProject,
    result: Option<Result<&'a ViewerProjectStatus, ViewerClientError>>,
) -> ProjectPresentation<'a> {
    let rendered = project
        .last_rendered_at
        .as_ref()
        .map(MachineTimestamp::display_minute);
    let project_status = match result {
        Some(Ok(status)) => status,
        pending_or_error => {
            let failed = pending_or_error.is_some();
            let signal = if failed {
                ProjectSignal::Unavailable(ProjectIssue::RequestFailed)
            } else {
                ProjectSignal::Loading
            };
            return ProjectPresentation {
                branch: None,
                comparison_base: None,
                review: if failed {
                    ProjectReview::StatusUnavailable
                } else {
                    ProjectReview::Loading
                },
                local: signal.clone(),
                ahead: signal,
                ahead_label: "Branch changes",
                issue: failed.then(|| "Git status unavailable".to_owned()),
                rendered,
            };
        }
    };
    let status = ProjectStatus::from_project(project_status);
    let branch = match &project_status.status {
        RepositoryStatus::Present {
            head: StatusHead::Branch { name, .. },
            ..
        } => Some(name.as_str()),
        RepositoryStatus::Present {
            head: StatusHead::Detached,
            ..
        } => Some("Detached HEAD"),
        RepositoryStatus::Present {
            head: StatusHead::Unavailable,
            ..
        }
        | RepositoryStatus::Absent => None,
    };
    let comparison_base = match &status.ahead {
        ProjectSignal::Ahead { base: Some(_), .. } => {
            Some(project_status.comparison_branch.as_ref())
        }
        _ => None,
    };
    let ahead_label = match project_status.branch_comparison {
        ViewerProjectBranchComparison::Upstream => "Unpushed commits",
        ViewerProjectBranchComparison::Branch { .. }
        | ViewerProjectBranchComparison::Unavailable { .. } => "Branch changes",
    };
    let issue = status.issue().map(|issue| issue.description().to_owned());
    ProjectPresentation {
        branch,
        comparison_base,
        review: status.review,
        local: status.local,
        ahead: status.ahead,
        ahead_label,
        issue,
        rendered,
    }
}

#[component]
pub(super) fn ReviewStatusDot(review: ProjectReview, #[props(default)] stale: bool) -> Element {
    let (label, classes) = match review {
        ProjectReview::Pending => ("Changes to review", "bg-acc"),
        ProjectReview::Clean => ("Up to date", "bg-add"),
        ProjectReview::ComparisonUnavailable => ("Comparison unavailable", "bg-warn"),
        ProjectReview::Loading => ("Loading Git status", "bg-ink-3"),
        ProjectReview::StatusUnavailable => ("Status unavailable", "bg-warn"),
        ProjectReview::Absent => ("Repository not found", "bg-warn"),
    };
    let (label, classes) = if stale {
        (
            "Git status is stale. Last successful values are shown. Retry Git status.",
            "bg-warn",
        )
    } else {
        (label, classes)
    };
    rsx! {
        span { class: "inline-flex size-3 shrink-0 items-center justify-center",
            if review == ProjectReview::Loading {
                span { role: "status", aria_label: "Loading Git status", ProjectStatusSpinner {} }
            } else {
                span {
                    class: "project-review-dot size-2 {classes}",
                    role: "img",
                    title: label,
                    aria_label: label,
                }
            }
        }
    }
}

#[component]
pub(super) fn ProjectSignalGlyph(signal: ProjectSignal) -> Element {
    rsx! {
        span {
            class: "inline-flex min-w-6 items-center font-medium tabular-nums {signal.text_classes()}",
            role: "img",
            title: signal.description().into_owned(),
            aria_label: signal.description().into_owned(),
            if signal == ProjectSignal::Loading {
                ProjectStatusSpinner {}
            } else {
                "{signal.glyph()}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_and_failed_statuses_never_offer_diffs_or_imply_a_clean_repository() {
        let project = ViewerProject {
            id: "GTL".try_into().unwrap(),
            name: "Git Tools".try_into().unwrap(),
            path: gtl_models::paths::RepositoryRoot::try_new("/repos/git-tools".into()).unwrap(),
            comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
            last_rendered_at: Some("2026-09-06T13:00:00Z".try_into().unwrap()),
        };
        let loading = project_presentation(&project, None);
        assert_eq!(loading.review, ProjectReview::Loading);
        assert_eq!(loading.local, ProjectSignal::Loading);
        assert!(!loading.local.is_available());
        assert!(!loading.ahead.has_changes());
        assert!(loading.issue.is_none());
        assert!(loading.rendered.is_some());
        let failed = project_presentation(&project, Some(Err(ViewerClientError::Internal)));
        assert_eq!(failed.review, ProjectReview::StatusUnavailable);
        assert!(!failed.local.is_available());
        assert!(!failed.ahead.has_changes());
        assert_eq!(failed.issue.as_deref(), Some("Git status unavailable"));
        assert_eq!(failed.rendered, loading.rendered);
    }
}

#[component]
fn ProjectStatusSpinner() -> Element {
    let ready = use_resource(|| async {
        dioxus_sdk_time::sleep(std::time::Duration::from_millis(150)).await;
        true
    });
    rsx! {
        span { class: "inline-flex size-3 items-center justify-center",
            if ready.read().as_ref() == Some(&true) {
                LoadingSpinner {}
            }
        }
    }
}
