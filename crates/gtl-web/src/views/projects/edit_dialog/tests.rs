use gtl_models::projects::comparison::ComparisonBranch;

use super::{
    ProjectEditBaseline, ProjectEditDraft, ProjectEditField, ProjectEditPlan,
    push_confirmation_projects,
};
use crate::test_support::{TestResult, project_name};

fn baseline(push_without_confirmation: Option<bool>) -> TestResult<ProjectEditBaseline> {
    Ok(ProjectEditBaseline {
        comparison_branch: ComparisonBranch::try_new("main")?,
        push_without_confirmation,
    })
}

fn draft(branch: &str, push_without_confirmation: Option<bool>) -> ProjectEditDraft {
    ProjectEditDraft {
        comparison_branch: branch.to_owned(),
        push_without_confirmation,
    }
}

#[test]
fn saved_values_plan_no_writes() -> TestResult {
    let baseline = baseline(Some(true))?;
    for draft in [draft("main", None), draft("main", Some(true))] {
        assert!(!draft.differs_from(&baseline));
        assert_eq!(draft.plan(&baseline), Ok(ProjectEditPlan::default()));
    }
    Ok(())
}

#[test]
fn changed_values_plan_only_their_writes() -> TestResult {
    let baseline = baseline(Some(false))?;
    let branch_only = draft("release/next", Some(false));
    assert!(branch_only.differs_from(&baseline));
    assert_eq!(
        branch_only.plan(&baseline),
        Ok(ProjectEditPlan {
            comparison_branch: Some(ComparisonBranch::try_new("release/next")?),
            push_without_confirmation: None,
        })
    );
    let push_only = draft("main", Some(true));
    assert!(push_only.differs_from(&baseline));
    assert_eq!(
        push_only.plan(&baseline),
        Ok(ProjectEditPlan {
            comparison_branch: None,
            push_without_confirmation: Some(true),
        })
    );
    Ok(())
}

#[test]
fn invalid_branches_reject_the_branch_field_without_planning() -> TestResult {
    let baseline = baseline(None)?;
    for branch in ["", "HEAD", "feature..next", "has space"] {
        let draft = draft(branch, Some(true));
        assert!(draft.differs_from(&baseline));
        let errors = draft
            .plan(&baseline)
            .err()
            .ok_or("invalid branch was planned")?;
        assert!(
            errors
                .message(
                    ProjectEditField::ComparisonBranch,
                    gtl_models::settings::ViewerLanguage::default()
                )
                .is_some(),
            "{branch}"
        );
    }
    Ok(())
}

#[test]
fn push_preference_adds_or_removes_only_the_edited_project() -> TestResult {
    let alpha = project_name("alpha")?;
    let beta = project_name("beta")?;
    assert_eq!(
        push_confirmation_projects(std::slice::from_ref(&alpha), &beta, true),
        [alpha.clone(), beta.clone()]
    );
    assert_eq!(
        push_confirmation_projects(&[alpha.clone(), beta.clone()], &beta, true),
        [alpha.clone(), beta.clone()]
    );
    assert_eq!(
        push_confirmation_projects(&[alpha.clone(), beta.clone()], &alpha, false),
        [beta]
    );
    Ok(())
}
