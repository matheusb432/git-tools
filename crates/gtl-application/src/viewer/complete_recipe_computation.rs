use std::sync::Arc;

use gtl_models::recipes::RecipeLabel;

use super::ViewerTabKind;
use crate::{
    diffs::View,
    recipes::{Recipe, RecipeLabelParts, recipe_label},
};

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteRecipeComputation {
    pub recipe: Recipe,
    pub kind: ViewerTabKind,
    pub view: View,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteRecipeComputationOk {
    pub label: RecipeLabel,
    pub view: Arc<View>,
}

#[cqrsy::command]
pub fn execute(command: CompleteRecipeComputation) -> CompleteRecipeComputationOk {
    let CompleteRecipeComputation { recipe, kind, view } = command;
    let rendered = || {
        let parts = RecipeLabelParts::from_view(&recipe, &view);
        recipe_label::rendered(&recipe, view.repo_name.clone(), &parts)
    };
    CompleteRecipeComputationOk {
        label: if kind == ViewerTabKind::Live {
            recipe_label::live(&recipe).unwrap_or_else(rendered)
        } else {
            rendered()
        },
        view: Arc::new(view),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        git::CommitCount,
        recipes::{RecipeLabel, RecipeLabelChanges},
    };

    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeTarget},
        utils::{
            diffs::commit,
            viewer::{empty_view, recipe},
        },
        viewer::complete_recipe_computation,
    };

    fn unpushed() -> Recipe {
        recipe(RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        })
    }

    fn published_label(recipe: Recipe, kind: ViewerTabKind) -> RecipeLabel {
        let mut view = empty_view();
        view.commits = vec![commit("abc1234")];
        complete_recipe_computation::execute(CompleteRecipeComputation { recipe, kind, view }).label
    }

    #[test]
    fn live_tabs_name_their_repository_while_snapshots_show_computed_changes() {
        let project = crate::utils::project_name("project");

        assert_eq!(
            published_label(unpushed(), ViewerTabKind::Live),
            RecipeLabel::Repository {
                repository: project.clone(),
            }
        );
        assert_eq!(
            published_label(unpushed(), ViewerTabKind::Snapshot),
            RecipeLabel::Changes {
                repository: project.clone(),
                changes: RecipeLabelChanges::UnpushedCommits {
                    count: CommitCount::new(1),
                },
            }
        );
        assert_eq!(
            published_label(
                recipe(RecipeOp::Diff {
                    target: RecipeTarget::Base {
                        rev: crate::utils::git_revision("v1"),
                    },
                }),
                ViewerTabKind::Live,
            ),
            RecipeLabel::Changes {
                repository: project,
                changes: RecipeLabelChanges::WorkingTree {
                    base: crate::utils::git_revision("v1"),
                },
            }
        );
    }

    #[test]
    fn explicit_name_overrides_the_computed_label() {
        let mut named = recipe(RecipeOp::MergeDiff {
            base: None,
            pinned: None,
        });
        named.name = Some(crate::utils::project_name("Release review"));

        assert_eq!(
            published_label(named, ViewerTabKind::Live),
            RecipeLabel::Named {
                name: crate::utils::project_name("Release review"),
            }
        );
    }
}
