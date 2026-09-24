use gtl_models::{
    failure::ProjectFailure,
    git::{BranchName, CommitCount, GitRefName},
    projects::comparison::ComparisonBranch,
    repository::status::{RepositoryStatus, StatusChanges, StatusHead, StatusUpstream},
};
use gtl_wire::viewer::projects::{ViewerProjectBranchComparison, ViewerProjectStatus};

use super::{ProjectIssue, ProjectReview, ProjectSignal, ProjectStatus};
use crate::test_support::TestResult;

fn viewer_project(
    status: RepositoryStatus,
    comparison: ViewerProjectBranchComparison,
) -> TestResult<ViewerProjectStatus> {
    Ok(ViewerProjectStatus {
        project_id: "GTL".try_into()?,
        status,
        comparison_branch: ComparisonBranch::default(),
        branch_comparison: comparison,
    })
}

fn tracking(ahead: u64, changes: StatusChanges) -> TestResult<RepositoryStatus> {
    Ok(RepositoryStatus::Present {
        head: StatusHead::Branch {
            name: BranchName::try_new("main".to_owned())?,
            upstream: StatusUpstream::Tracking {
                reference: GitRefName::try_new("refs/remotes/origin/main".to_owned())?,
                ahead: CommitCount::new(ahead),
            },
        },
        changes,
    })
}

#[test]
fn unavailable_comparison_preserves_reason_and_local_action() -> TestResult {
    let project = viewer_project(
        tracking(0, StatusChanges::Clean)?,
        ViewerProjectBranchComparison::Unavailable {
            failure: ProjectFailure::CommitCountUnavailable.into(),
        },
    )?;
    let status = ProjectStatus::from_project(&project);
    assert_eq!(status.review, ProjectReview::ComparisonUnavailable);
    assert!(status.local.is_available());
    assert!(!status.ahead.is_available());
    assert!(!status.ahead.has_unpushed_commits());
    assert_eq!(
        status.issue(),
        Some(&ProjectIssue::ComparisonUnavailable(
            ProjectFailure::CommitCountUnavailable.into()
        ))
    );
    Ok(())
}

#[test]
fn fallback_comparison_uses_its_count_and_base_instead_of_upstream() -> TestResult {
    let project = viewer_project(
        tracking(0, StatusChanges::Clean)?,
        ViewerProjectBranchComparison::Branch {
            commits_ahead: CommitCount::new(2),
        },
    )?;
    let status = ProjectStatus::from_project(&project);
    assert_eq!(status.review, ProjectReview::Pending);
    assert_eq!(
        status.ahead,
        ProjectSignal::Ahead {
            count: CommitCount::new(2),
            base: Some(project.comparison_branch)
        }
    );
    assert!(status.issue().is_none());
    assert!(!status.ahead.has_unpushed_commits());
    Ok(())
}

#[test]
fn push_requires_a_positive_upstream_count() -> TestResult {
    for (count, pushable) in [(0, false), (1, true), (12, true)] {
        let project = viewer_project(
            tracking(count, StatusChanges::Clean)?,
            ViewerProjectBranchComparison::Upstream,
        )?;
        assert_eq!(
            ProjectStatus::from_project(&project)
                .ahead
                .has_unpushed_commits(),
            pushable,
        );
    }
    assert!(!ProjectSignal::Loading.has_unpushed_commits());
    Ok(())
}

#[test]
fn absent_repository_disables_both_actions() -> TestResult {
    let project = viewer_project(
        RepositoryStatus::Absent,
        ViewerProjectBranchComparison::Upstream,
    )?;
    let status = ProjectStatus::from_project(&project);
    assert_eq!(status.review, ProjectReview::Absent);
    assert!(!status.local.is_available());
    assert!(!status.ahead.is_available());
    assert_eq!(status.issue(), Some(&ProjectIssue::RepositoryAbsent));
    Ok(())
}

#[test]
fn only_the_owning_projects_setting_offers_a_branch_change() -> TestResult {
    let project = gtl_models::paths::RepositoryRoot::try_new("/repos/project".into())?;
    let primary = gtl_models::paths::RepositoryRoot::try_new("/repos/primary".into())?;
    let missing = |owner: Option<gtl_models::paths::RepositoryRoot>| {
        ProjectIssue::ComparisonUnavailable(
            ProjectFailure::ComparisonBranchMissing {
                path: project.clone(),
                branch: ComparisonBranch::default(),
                project: owner,
            }
            .into(),
        )
    };

    assert!(missing(Some(project.clone())).comparison_branch_resolves(&project));
    assert!(!missing(Some(primary)).comparison_branch_resolves(&project));
    assert!(!missing(None).comparison_branch_resolves(&project));
    assert!(
        !ProjectIssue::ComparisonUnavailable(ProjectFailure::CommitCountUnavailable.into())
            .comparison_branch_resolves(&project)
    );
    Ok(())
}
