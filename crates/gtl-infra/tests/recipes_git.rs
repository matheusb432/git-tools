#![cfg(test)]

use std::{path::Path, process::Command};

use gtl_application::{
    recipes::{RecipeOp, RecipeTarget},
    repositories::build_recipes::{self, BuildRepositoryRecipes},
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::repository::traversal::RepositoryTraversalScope;

fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed in {}",
        repository.display()
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn init_repo(repository: &Path) {
    std::fs::create_dir_all(repository).unwrap();
    git(repository, &["init", "-q", "-b", "main"]);
    git(
        repository,
        &["config", "user.email", "test@example.invalid"],
    );
    git(repository, &["config", "user.name", "Test"]);
    std::fs::write(repository.join("first.txt"), "first\n").unwrap();
    git(repository, &["add", "."]);
    git(repository, &["commit", "-qm", "first"]);
}

fn init_repo_with_unpushed_commit(repository: &Path) -> (String, String) {
    init_repo(repository);

    let origin_bare = repository.join("origin.git");
    git(
        repository,
        &["init", "--bare", "-q", origin_bare.to_str().unwrap()],
    );
    git(
        repository,
        &["remote", "add", "origin", origin_bare.to_str().unwrap()],
    );
    git(repository, &["push", "-q", "-u", "origin", "main"]);
    let base_sha = git(repository, &["rev-parse", "HEAD"]);

    std::fs::write(repository.join("second.txt"), "second\n").unwrap();
    git(repository, &["add", "."]);
    git(repository, &["commit", "-qm", "second"]);
    let head_sha = git(repository, &["rev-parse", "HEAD"]);
    (base_sha, head_sha)
}

#[test]
fn real_git_repository_build_pins_each_repository_independently() {
    let temporary = tempfile::tempdir().unwrap();
    let api = temporary.path().join("api");
    let (api_base_sha, api_head_sha) = init_repo_with_unpushed_commit(&api);
    let web = temporary.path().join("web");
    init_repo(&web);

    let recipes = build_recipes::execute(
        BuildRepositoryRecipes {
            root: temporary.path().into(),
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
