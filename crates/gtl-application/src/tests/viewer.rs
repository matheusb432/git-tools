use gtl_wire::recipes::{Recipe, RecipeOp, RecipeSource};

use crate::diffs::View;

pub(crate) fn recipe(op: RecipeOp) -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo("/repos/project".into()),
        op,
        name: None,
    }
}

pub(crate) fn empty_view() -> View {
    let mut view = super::diffs::view();
    view.repo_name = "project".into();
    view.repo_root = "/repos/project".into();
    view.upstream = "main".into();
    view.title = "viewer".into();
    view.cmd.range = "main..HEAD".into();
    view.commits_label.clear();
    view.foot.cmd.clear();
    view
}
