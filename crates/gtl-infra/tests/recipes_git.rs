#![cfg(test)]

use std::path::Path;

use gtl_application::{
    recipes::{RecipeOp, RecipeTarget},
    repositories::build_recipes::{self, BuildRepositoryRecipes},
};
use gtl_infra::{git_client::HybridGitClient, testing::TestRepository};
use gtl_models::repository::traversal::RepositoryTraversalScope;

fn committed_repository(path: &Path) -> TestRepository {
    let repository = TestRepository::init(path);
    repository.write("first.txt", "first\n");
    repository.commit_all("first");
    repository
}

#[test]
fn real_git_repository_build_pins_each_repository_independently() {
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    let api = committed_repository(&workspace.join("api"));
    api.add_bare_origin(temporary.path().join("origin.git"));
    let api_base_sha = api.git(&["rev-parse", "HEAD"]);
    api.write("second.txt", "second\n");
    let api_head_sha = api.commit_all("second");
    committed_repository(&workspace.join("web"));

    let recipes = build_recipes::execute(
        BuildRepositoryRecipes {
            root: workspace,
            operation: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        },
        &HybridGitClient,
        &gtl_application::utils::ProjectComparisons::default(),
    )
    .unwrap();
    let json = serde_json::to_value(recipes.recipes).unwrap();

    assert_eq!(json.as_array().unwrap().len(), 2);
    assert_eq!(json[0]["name"], "api");
    assert_eq!(json[0]["op"]["target"]["pinned"]["base"], api_base_sha);
    assert_eq!(json[0]["op"]["target"]["pinned"]["head"], api_head_sha);
    assert_eq!(json[1]["name"], "web");
    assert!(json[1]["op"]["target"]["pinned"].is_null());
}
