use gtl_models::recipes::RecipeLabel;

use crate::{
    diffs::View,
    recipes::{Recipe, RecipeOp, RecipeSource},
};

/// Names a test tab with a distinguishable label.
pub(crate) fn label(name: &str) -> RecipeLabel {
    RecipeLabel::Named {
        name: crate::utils::project_name(name),
    }
}

pub(crate) fn recipe(op: RecipeOp) -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(crate::utils::repository_root("/repos/project")),
        op,
        name: None,
    }
}

pub(crate) fn empty_view() -> View {
    let mut view = super::diffs::view();
    view.repo_name = crate::utils::project_name("project");
    view.repo_root = crate::utils::repository_root("/repos/project");
    view.upstream = crate::utils::git_revision("main");
    view.title = super::diffs::view_title("viewer");
    view.cmd.range = "main..HEAD".into();
    view.foot.cmd.clear();
    view
}
