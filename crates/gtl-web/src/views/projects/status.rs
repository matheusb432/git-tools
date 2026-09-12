use gtl_models::{
    git::CommitCount,
    projects::comparison::ComparisonBranch,
    repository::{
        PathCount,
        status::{RepositoryStatus, StatusChanges, StatusClass, StatusHead, StatusUpstream},
    },
};
use gtl_wire::viewer::projects::{ViewerProject, ViewerProjectBranchComparison};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProjectIssue {
    RepositoryAbsent,
    HeadUnavailable,
    WorkingTreeUnavailable,
    UpstreamMissing,
    ComparisonUnavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProjectSignal {
    Local {
        tracked: PathCount,
        untracked: PathCount,
    },
    Ahead {
        count: CommitCount,
        base: Option<ComparisonBranch>,
    },
    Unavailable(ProjectIssue),
}

impl ProjectSignal {
    pub(super) fn has_changes(&self) -> bool {
        match self {
            Self::Ahead { count, .. } => count.into_inner() > 0,
            Self::Local { tracked, untracked } => !tracked.is_zero() || !untracked.is_zero(),
            Self::Unavailable(_) => false,
        }
    }

    pub(super) const fn is_available(&self) -> bool {
        !matches!(self, Self::Unavailable(_))
    }

    pub(super) const fn issue(&self) -> Option<&ProjectIssue> {
        match self {
            Self::Unavailable(issue) => Some(issue),
            Self::Local { .. } | Self::Ahead { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectReview {
    Pending,
    Clean,
    ComparisonUnavailable,
    StatusUnavailable,
    Absent,
}

pub(super) struct ProjectStatus {
    pub(super) local: ProjectSignal,
    pub(super) ahead: ProjectSignal,
    pub(super) review: ProjectReview,
}

impl ProjectStatus {
    pub(super) fn from_project(project: &ViewerProject) -> Self {
        let local = match &project.status {
            RepositoryStatus::Absent => ProjectSignal::Unavailable(ProjectIssue::RepositoryAbsent),
            RepositoryStatus::Present { changes, .. } => match *changes {
                StatusChanges::Unavailable => {
                    ProjectSignal::Unavailable(ProjectIssue::WorkingTreeUnavailable)
                }
                StatusChanges::Clean => ProjectSignal::Local {
                    tracked: PathCount::new(0),
                    untracked: PathCount::new(0),
                },
                StatusChanges::Changed { tracked, untracked } => {
                    ProjectSignal::Local { tracked, untracked }
                }
            },
        };
        let ahead = match &project.branch_comparison {
            ViewerProjectBranchComparison::Upstream => upstream_signal(&project.status),
            ViewerProjectBranchComparison::Branch { commits_ahead } => ProjectSignal::Ahead {
                count: *commits_ahead,
                base: Some(project.comparison_branch.clone()),
            },
            ViewerProjectBranchComparison::Unavailable { reason } => {
                ProjectSignal::Unavailable(ProjectIssue::ComparisonUnavailable(reason.clone()))
            }
        };
        let review = match project.review_class() {
            StatusClass::Pending => ProjectReview::Pending,
            StatusClass::Clean => ProjectReview::Clean,
            StatusClass::Warn if local.is_available() && !ahead.is_available() => {
                ProjectReview::ComparisonUnavailable
            }
            StatusClass::Warn => ProjectReview::StatusUnavailable,
            StatusClass::Absent => ProjectReview::Absent,
        };
        Self {
            local,
            ahead,
            review,
        }
    }

    pub(super) fn issue(&self) -> Option<&ProjectIssue> {
        match self.review {
            ProjectReview::ComparisonUnavailable
            | ProjectReview::StatusUnavailable
            | ProjectReview::Absent => self.ahead.issue().or_else(|| self.local.issue()),
            ProjectReview::Pending | ProjectReview::Clean => None,
        }
    }
}

fn upstream_signal(status: &RepositoryStatus) -> ProjectSignal {
    let issue = match status {
        RepositoryStatus::Absent => ProjectIssue::RepositoryAbsent,
        RepositoryStatus::Present { head, .. } => match head {
            StatusHead::Unavailable => ProjectIssue::HeadUnavailable,
            StatusHead::Detached
            | StatusHead::Branch {
                upstream: StatusUpstream::Missing,
                ..
            } => ProjectIssue::UpstreamMissing,
            StatusHead::Branch {
                upstream: StatusUpstream::Tracking { ahead, .. },
                ..
            } => {
                return ProjectSignal::Ahead {
                    count: *ahead,
                    base: None,
                };
            }
        },
    };
    ProjectSignal::Unavailable(issue)
}

#[cfg(test)]
mod tests;
