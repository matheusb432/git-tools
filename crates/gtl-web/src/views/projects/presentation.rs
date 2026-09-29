use std::borrow::Cow;

use dioxus::prelude::*;
use gtl_models::{
    repository::status::{RepositoryStatus, StatusHead},
    settings::ViewerLanguage,
};
use gtl_wire::viewer::projects::ViewerProjectStatus;

use super::status::{ProjectIssue, ProjectReview, ProjectSignal, ProjectStatus};
use crate::shared::{
    failure_message::failure_message,
    i18n::{t, use_language},
    ui::LoadingSpinner,
    viewer_client::ViewerClientError,
};

impl ProjectIssue {
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
    pub(super) review: ProjectReview,
    pub(super) local: ProjectSignal,
    pub(super) ahead: ProjectSignal,
}

pub(super) fn project_presentation(
    result: Option<Result<&ViewerProjectStatus, ViewerClientError>>,
    language: ViewerLanguage,
) -> ProjectPresentation<'_> {
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
                review: if failed {
                    ProjectReview::StatusUnavailable
                } else {
                    ProjectReview::Loading
                },
                local: signal.clone(),
                ahead: signal,
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
    ProjectPresentation {
        branch,
        review: status.review,
        local: status.local,
        ahead: status.ahead,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_and_failed_statuses_never_offer_diffs_or_imply_a_clean_repository() {
        let loading = project_presentation(None, ViewerLanguage::EnUs);
        assert_eq!(loading.review, ProjectReview::Loading);
        assert_eq!(loading.local, ProjectSignal::Loading);
        assert!(!loading.local.is_available());
        assert!(!loading.ahead.has_changes());
        let failed = project_presentation(
            Some(Err(ViewerClientError::InvalidMessage)),
            ViewerLanguage::EnUs,
        );
        assert_eq!(failed.review, ProjectReview::StatusUnavailable);
        assert!(!failed.local.is_available());
        assert!(!failed.ahead.has_changes());
        assert_eq!(failed.ahead.issue(), Some(&ProjectIssue::RequestFailed));
    }
}

#[component]
pub(super) fn ProjectStatusSpinner() -> Element {
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
