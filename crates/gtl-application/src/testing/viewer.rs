use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource};

use crate::diffs::{Cmd, Foot, View};

pub(crate) fn recipe(op: RecipeOp) -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo("/repos/project".into()),
        op,
        name: None,
    }
}

pub(crate) fn empty_view() -> View {
    View {
        exclusions: None,
        repo_name: "project".into(),
        repo_root: "/repos/project".into(),
        branch: "feature".into(),
        upstream: "main".into(),
        commits: Vec::new(),
        files: Vec::new(),
        title: "viewer".into(),
        cmd: Cmd {
            lead: String::new(),
            range: "main..HEAD".into(),
            trail: String::new(),
        },
        commits_label: String::new(),
        foot: Foot {
            cmd: String::new(),
            note: String::new(),
        },
    }
}
