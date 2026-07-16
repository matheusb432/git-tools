use std::{num::NonZeroU32, path::Path, process::Command};

use application::{
    diffs::DiffTarget,
    recipes::{
        RecipeRequest,
        build_subrepos::{self, BuildSubrepoRecipes},
        pin::{self, PinRecipe},
    },
};
use infra::{git_runner::StdGitRunner, repo_discovery::WalkdirRepoDiscovery};

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git starts");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_string()
}

fn init_repo(repo: &Path) {
    std::fs::create_dir_all(repo).unwrap();
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["config", "user.email", "test@example.invalid"]);
    git(repo, &["config", "user.name", "Test"]);
    std::fs::write(repo.join("first.txt"), "first\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "first"]);
}

fn init_repo_with_unpushed_commit(repo: &Path) -> (String, String) {
    init_repo(repo);

    let origin_bare = repo.join("origin.git");
    git(
        repo,
        &["init", "--bare", "-q", origin_bare.to_str().unwrap()],
    );
    git(
        repo,
        &["remote", "add", "origin", origin_bare.to_str().unwrap()],
    );
    git(repo, &["push", "-q", "-u", "origin", "main"]);
    let base_sha = git(repo, &["rev-parse", "HEAD"]);

    std::fs::write(repo.join("second.txt"), "second\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "second"]);
    let head_sha = git(repo, &["rev-parse", "HEAD"]);
    (base_sha, head_sha)
}

fn pin_json(repo: &Path, operation: RecipeRequest) -> serde_json::Value {
    let recipe = pin::execute(
        PinRecipe {
            repo: repo.into(),
            operation,
            name: Some("repo".into()),
        },
        &StdGitRunner,
    )
    .expect("recipe builds through real Git");
    serde_json::to_value(recipe).unwrap()
}

#[test]
fn real_git_runner_pins_an_unpushed_snapshot_recipe() {
    let temp = tempfile::tempdir().unwrap();
    let (base_sha, head_sha) = init_repo_with_unpushed_commit(temp.path());

    let json = pin_json(
        temp.path(),
        RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
    );

    let canonical = std::fs::canonicalize(temp.path()).unwrap();
    assert_eq!(
        json["source"]["value"].as_str(),
        Some(canonical.to_string_lossy().as_ref())
    );
    assert_eq!(json["name"], "repo");
    assert_eq!(json["op"]["op"], "diff");
    assert_eq!(json["op"]["target"]["target"], "unpushed");
    assert_eq!(json["op"]["target"]["pinned"]["base"], base_sha);
    assert_eq!(json["op"]["target"]["pinned"]["head"], head_sha);
}

#[test]
fn real_git_runner_keeps_unpushed_symbolic_without_an_upstream() {
    let temp = tempfile::tempdir().unwrap();
    init_repo(temp.path());

    let json = pin_json(
        temp.path(),
        RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
    );

    assert_eq!(json["op"]["target"]["target"], "unpushed");
    assert!(json["op"]["target"]["pinned"].is_null());
}

#[test]
fn real_git_runner_pins_last_n_to_the_resolved_endpoints() {
    let temp = tempfile::tempdir().unwrap();
    let (_, head_sha) = init_repo_with_unpushed_commit(temp.path());
    let base_sha = git(temp.path(), &["rev-parse", "HEAD~1"]);

    let json = pin_json(
        temp.path(),
        RecipeRequest::Diff(DiffTarget::Last {
            count: NonZeroU32::new(1).unwrap(),
            pinned: None,
        }),
    );

    assert_eq!(json["op"]["target"]["target"], "last");
    assert_eq!(json["op"]["target"]["count"], 1);
    assert_eq!(json["op"]["target"]["pinned"]["base"], base_sha);
    assert_eq!(json["op"]["target"]["pinned"]["head"], head_sha);
}

#[test]
fn real_git_runner_pins_two_dot_range_and_keeps_three_dot_symbolic() {
    let temp = tempfile::tempdir().unwrap();
    let (base_sha, head_sha) = init_repo_with_unpushed_commit(temp.path());
    let two_dot = format!("{base_sha}..{head_sha}");

    let pinned = pin_json(
        temp.path(),
        RecipeRequest::Diff(DiffTarget::Range {
            range: two_dot.clone(),
            pinned: None,
        }),
    );
    assert_eq!(pinned["op"]["target"]["range"], two_dot);
    assert_eq!(pinned["op"]["target"]["pinned"]["base"], base_sha);
    assert_eq!(pinned["op"]["target"]["pinned"]["head"], head_sha);

    let three_dot = format!("{base_sha}...{head_sha}");
    let symbolic = pin_json(
        temp.path(),
        RecipeRequest::Diff(DiffTarget::Range {
            range: three_dot.clone(),
            pinned: None,
        }),
    );
    assert_eq!(symbolic["op"]["target"]["range"], three_dot);
    assert!(symbolic["op"]["target"]["pinned"].is_null());
}

#[test]
fn real_git_runner_pins_merge_diff_to_merge_base_and_head() {
    let temp = tempfile::tempdir().unwrap();
    init_repo(temp.path());
    git(temp.path(), &["checkout", "-qb", "feature"]);
    std::fs::write(temp.path().join("feature.txt"), "feature\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feature"]);
    let base_sha = git(temp.path(), &["merge-base", "main", "HEAD"]);
    let head_sha = git(temp.path(), &["rev-parse", "HEAD"]);

    let json = pin_json(temp.path(), RecipeRequest::MergeDiff { base: None });

    assert_eq!(json["op"]["op"], "merge_diff");
    assert!(json["op"]["base"].is_null());
    assert_eq!(json["op"]["pinned"]["base"], base_sha);
    assert_eq!(json["op"]["pinned"]["head"], head_sha);
}

#[test]
fn real_git_runner_pins_squash_preview_like_unpushed() {
    let temp = tempfile::tempdir().unwrap();
    let (base_sha, head_sha) = init_repo_with_unpushed_commit(temp.path());

    let json = pin_json(temp.path(), RecipeRequest::SquashPreview);

    assert_eq!(json["op"]["op"], "squash_preview");
    assert_eq!(json["op"]["pinned"]["base"], base_sha);
    assert_eq!(json["op"]["pinned"]["head"], head_sha);
}

#[test]
fn real_git_subrepo_build_pins_each_repository_independently() {
    let temp = tempfile::tempdir().unwrap();
    let api = temp.path().join("api");
    let (api_base_sha, api_head_sha) = init_repo_with_unpushed_commit(&api);
    let web = temp.path().join("web");
    init_repo(&web);

    let recipes = build_subrepos::execute(
        BuildSubrepoRecipes {
            root: temp.path().into(),
            operation: RecipeRequest::Diff(DiffTarget::Unpushed { pinned: None }),
            include_worktrees: false,
        },
        &WalkdirRepoDiscovery,
        &StdGitRunner,
    )
    .expect("subrepo recipes build through real Git");
    let json = serde_json::to_value(recipes).unwrap();

    assert_eq!(json.as_array().unwrap().len(), 2);
    assert_eq!(json[0]["name"], "api");
    assert_eq!(json[0]["op"]["target"]["pinned"]["base"], api_base_sha);
    assert_eq!(json[0]["op"]["target"]["pinned"]["head"], api_head_sha);
    assert_eq!(json[1]["name"], "web");
    assert!(json[1]["op"]["target"]["pinned"].is_null());
}
