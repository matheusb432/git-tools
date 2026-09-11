use std::borrow::Cow;

use dioxus::prelude::*;
use gtl_models::{
    repository::status::{RepositoryStatus, StatusChanges, StatusClass, StatusHead},
    timestamps::MachineTimestamp,
};
use gtl_wire::viewer::projects::{ViewerProject, ViewerProjectBranchComparison};

use super::{
    loading::ProjectLoad,
    status::{ProjectIssue, ProjectReview, ProjectSignal, ProjectStatus},
};

impl ProjectIssue {
    pub(super) fn description(&self) -> &str {
        match self {
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
            Self::Unavailable(_) => "-".to_owned(),
        }
    }

    pub(super) fn text_classes(&self) -> &'static str {
        match self {
            Self::Local { tracked, untracked } if !tracked.is_zero() || !untracked.is_zero() => {
                "text-acc"
            }
            Self::Ahead { count, .. } if count.into_inner() > 0 => "text-acc",
            Self::Local { .. } | Self::Ahead { .. } => "text-ink-3",
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

pub(super) fn project_presentation(project: &ViewerProject) -> ProjectPresentation<'_> {
    let status = ProjectStatus::from_project(project);
    let branch = match &project.status {
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
        ProjectSignal::Ahead { base: Some(_), .. } => Some(project.comparison_branch.as_ref()),
        _ => None,
    };
    let ahead_label = match project.branch_comparison {
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
        rendered: project
            .last_rendered_at
            .as_ref()
            .map(MachineTimestamp::display_minute),
    }
}

#[component]
pub(super) fn ReviewStatusDot(review: ProjectReview) -> Element {
    let (label, classes) = match review {
        ProjectReview::Pending => ("Changes to review", "bg-acc"),
        ProjectReview::Clean => ("Up to date", "bg-add"),
        ProjectReview::ComparisonUnavailable => ("Comparison unavailable", "bg-warn"),
        ProjectReview::StatusUnavailable => ("Status unavailable", "bg-warn"),
        ProjectReview::Absent => ("Repository not found", "bg-warn"),
    };
    rsx! {
        span {
            class: "project-review-dot size-2 {classes}",
            role: "img",
            title: label,
            aria_label: label,
        }
    }
}

#[component]
pub(super) fn ProjectSignalGlyph(signal: ProjectSignal) -> Element {
    rsx! {
        span {
            class: "font-medium tabular-nums {signal.text_classes()}",
            role: "img",
            title: signal.description().into_owned(),
            aria_label: signal.description().into_owned(),
            "{signal.glyph()}"
        }
    }
}

#[component]
pub(super) fn ProjectsSummary(load: ReadSignal<ProjectLoad>) -> Element {
    let load = load.read();
    let summary = load.projects.as_ref().map_or_else(
        || "Loading projects".to_owned(),
        |projects| {
            let total = projects.len();
            let to_review = projects
                .iter()
                .filter(|project| project.review_class() == StatusClass::Pending)
                .count();
            let noun = if total == 1 { "project" } else { "projects" };
            match to_review {
                0 => format!("{total} {noun}, nothing to review"),
                to_review => format!("{total} {noun}, {to_review} to review"),
            }
        },
    );
    rsx! {
        p { class: "min-w-0 truncate text-ink-2 max-sm:hidden", "{summary}" }
    }
}
