use std::path::PathBuf;

use gtl_models::{
    git::{BranchName, CommitCount, GitRefName},
    paths::RepositoryRoot,
    projects::comparison::ComparisonBranch,
    repository::status::{RepositoryStatus, StatusChanges, StatusHead, StatusUpstream},
};
use gtl_wire::viewer::projects::{ViewerProject, ViewerProjectBranchComparison};

use super::{ProjectIssue, ProjectReview, ProjectSignal, ProjectStatus};
use crate::test_support::{TestResult, project_name};

fn repository_root(path: &str) -> TestResult<RepositoryRoot> {
    Ok(RepositoryRoot::try_new(PathBuf::from(path))?)
}

fn viewer_project(
    status: RepositoryStatus,
    comparison: ViewerProjectBranchComparison,
) -> TestResult<ViewerProject> {
    Ok(ViewerProject {
        path: repository_root("/home/dev/tools/git-tools")?,
        name: project_name("git-tools")?,
        status,
        last_rendered_at: None,
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
            reason: "missing comparison branch".to_owned(),
        },
    )?;
    let status = ProjectStatus::from_project(&project);
    assert_eq!(status.review, ProjectReview::ComparisonUnavailable);
    assert!(status.local.is_available());
    assert!(!status.ahead.is_available());
    assert_eq!(
        status.issue(),
        Some(&ProjectIssue::ComparisonUnavailable(
            "missing comparison branch".to_owned()
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
