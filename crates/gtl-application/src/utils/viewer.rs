use crate::{
    diffs::View,
    recipes::{Recipe, RecipeOp, RecipeSource},
};

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
    view.title = "viewer".into();
    view.cmd.range = "main..HEAD".into();
    view.commits_label.clear();
    view.foot.cmd.clear();
    view
}
