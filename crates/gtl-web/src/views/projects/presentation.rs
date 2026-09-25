use std::borrow::Cow;

use dioxus::prelude::*;
use gtl_models::{
    failure::Failure,
    paths::RepositoryRoot,
    repository::status::{RepositoryStatus, StatusChanges, StatusHead},
    settings::ViewerLanguage,
};
use gtl_wire::viewer::projects::{
    ViewerProject, ViewerProjectBranchComparison, ViewerProjectStatus,
};

use super::status::{ProjectIssue, ProjectReview, ProjectSignal, ProjectStatus};
use crate::shared::{
    failure_message::failure_message,
    i18n::{t, use_language},
    ui::LoadingSpinner,
    viewer_client::ViewerClientError,
};

impl ProjectIssue {
    /// Whether a different comparison branch in `project`'s own setting can resolve the issue.
    pub(super) fn comparison_branch_resolves(&self, project: &RepositoryRoot) -> bool {
        matches!(
            self,
            Self::ComparisonUnavailable(Failure::Project(failure))
                if failure.comparison_setting().is_some_and(|(owner, _)| owner == project)
        )
    }

    pub(super) fn description(&self, language: ViewerLanguage) -> String {
        match self {
            Self::RequestFailed => t!(language, "projects-issue-request-failed"),
            Self::RepositoryAbsent => t!(language, "projects-issue-repository-absent"),
            Self::HeadUnavailable => t!(language, "projects-issue-head-unavailable"),
            Self::WorkingTreeUnavailable => {
                t!(language, "projects-issue-working-tree-unavailable")
            }
            Self::UpstreamMissing => t!(language, "projects-issue-upstream-missing"),
            Self::ComparisonUnavailable(failure) => failure_message(failure, language),
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

    pub(super) fn description(&self, language: ViewerLanguage) -> String {
        match self {
            Self::Local { tracked, untracked } => {
                match (!tracked.is_zero(), !untracked.is_zero()) {
                    (true, true) => t!(language, "projects-local-modified-untracked"),
                    (true, false) => t!(language, "projects-local-modified"),
                    (false, true) => t!(language, "projects-local-untracked"),
                    (false, false) => t!(language, "projects-local-clean"),
                }
            }
            Self::Ahead { count, base: None } => {
                t!(
                    language,
                    "projects-ahead-upstream",
                    count = count.into_inner()
                )
            }
            Self::Ahead {
                count,
                base: Some(base),
            } => t!(
                language,
                "projects-ahead-branch",
                count = count.into_inner(),
                base = base.to_string()
            ),
            Self::Loading => t!(language, "projects-status-loading"),
            Self::Unavailable(issue) => issue.description(language),
        }
    }
}

pub(super) struct ProjectPresentation<'a> {
    pub(super) branch: Option<Cow<'a, str>>,
    pub(super) comparison_base: Option<&'a str>,
    pub(super) review: ProjectReview,
    pub(super) local: ProjectSignal,
    pub(super) ahead: ProjectSignal,
    pub(super) ahead_label: String,
    pub(super) issue: Option<String>,
    /// The issue names a comparison branch that this project's setting controls.
    pub(super) comparison_branch_fix: bool,
}

pub(super) fn project_presentation<'a>(
    project: &ViewerProject,
    result: Option<Result<&'a ViewerProjectStatus, ViewerClientError>>,
    language: ViewerLanguage,
) -> ProjectPresentation<'a> {
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
                ahead_label: t!(language, "comparison-branch-changes"),
                issue: failed.then(|| t!(language, "projects-issue-request-failed")),
                comparison_branch_fix: false,
            };
        }
    };
    let status = ProjectStatus::from_project(project_status);
    let branch = match &project_status.status {
        RepositoryStatus::Present {
            head: StatusHead::Branch { name, .. },
            ..
        } => Some(Cow::Borrowed(name.as_str())),
        RepositoryStatus::Present {
            head: StatusHead::Detached,
            ..
        } => Some(Cow::Owned(t!(language, "projects-detached-head"))),
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
        ViewerProjectBranchComparison::Upstream => t!(language, "comparison-unpushed-commits"),
        ViewerProjectBranchComparison::Branch { .. }
        | ViewerProjectBranchComparison::Unavailable { .. } => {
            t!(language, "comparison-branch-changes")
        }
    };
    let issue = status.issue().map(|issue| issue.description(language));
    let comparison_branch_fix = status
        .issue()
        .is_some_and(|issue| issue.comparison_branch_resolves(&project.path));
    ProjectPresentation {
        branch,
        comparison_base,
        review: status.review,
        local: status.local,
        ahead: status.ahead,
        ahead_label,
        issue,
        comparison_branch_fix,
    }
}

#[component]
pub(super) fn ReviewStatusDot(review: ProjectReview, #[props(default)] stale: bool) -> Element {
    let language = use_language();
    let (label, classes) = match review {
        ProjectReview::Pending => (t!(language, "projects-review-pending"), "bg-acc"),
        ProjectReview::Clean => (t!(language, "projects-review-clean"), "bg-add"),
        ProjectReview::ComparisonUnavailable => (
            t!(language, "projects-review-comparison-unavailable"),
            "bg-warn",
        ),
        ProjectReview::Loading => (t!(language, "projects-status-loading"), "bg-ink-3"),
        ProjectReview::StatusUnavailable => (
            t!(language, "projects-review-status-unavailable"),
            "bg-warn",
        ),
        ProjectReview::Absent => (t!(language, "projects-issue-repository-absent"), "bg-warn"),
    };
    let (label, classes) = if stale {
        (t!(language, "projects-review-stale"), "bg-warn")
    } else {
        (label, classes)
    };
    rsx! {
        span { class: "inline-flex size-3 shrink-0 items-center justify-center",
            if review == ProjectReview::Loading {
                span {
                    role: "status",
                    aria_label: t!(language, "projects-status-loading"),
                    ProjectStatusSpinner {}
                }
            } else {
                span {
                    class: "project-review-dot size-2 {classes}",
                    role: "img",
                    title: label.clone(),
                    aria_label: label,
                }
            }
        }
    }
}

#[component]
pub(super) fn ProjectSignalGlyph(signal: ProjectSignal) -> Element {
    let description = signal.description(use_language());
    rsx! {
        span {
            class: "inline-flex min-w-6 items-center font-medium tabular-nums {signal.text_classes()}",
            role: "img",
            title: description.clone(),
            aria_label: description,
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
        let loading = project_presentation(&project, None, ViewerLanguage::EnUs);
        assert_eq!(loading.review, ProjectReview::Loading);
        assert_eq!(loading.local, ProjectSignal::Loading);
        assert!(!loading.local.is_available());
        assert!(!loading.ahead.has_changes());
        assert!(loading.issue.is_none());
        let failed = project_presentation(
            &project,
            Some(Err(ViewerClientError::InvalidMessage)),
            ViewerLanguage::EnUs,
        );
        assert_eq!(failed.review, ProjectReview::StatusUnavailable);
        assert!(!failed.local.is_available());
        assert!(!failed.ahead.has_changes());
        assert_eq!(failed.issue.as_deref(), Some("Git status unavailable"));
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
