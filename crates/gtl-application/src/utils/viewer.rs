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
        source: RecipeSource::LocalRepo {
            root: crate::utils::repository_root("//fixture.invalid/repositories/repos/project"),
            op,
        },
        name: None,
    }
}

pub(crate) fn empty_view() -> View {
    let mut view = super::diffs::view();
    let repository = super::diffs::repository_origin_mut(&mut view);
    repository.name = crate::utils::project_name("project");
    repository.root = crate::utils::repository_root("//fixture.invalid/repositories/repos/project");
    repository.upstream = crate::utils::git_revision("main");
    view.title = super::diffs::view_title("viewer");
    view.cmd.range = "main..HEAD".into();
    view.foot.cmd.clear();
    view
}

/// Moves a repository recipe to `root`, keeping its operation and name.
pub(crate) fn set_root(recipe: &mut Recipe, root: gtl_models::paths::RepositoryRoot) {
    if let RecipeSource::LocalRepo { root: current, .. } = &mut recipe.source {
        *current = root;
    }
}
