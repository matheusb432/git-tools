//! Real-git integration tests for the diff engine (`gtl_application::diffs`) driven
//! through the `HybridGitClient` adapter. The pure parsing paths are unit-tested in
//! `application`; here we prove the engine against actual `git log`/`git blame`
//! output on fixture repos.

use std::{path::Path, process::Command};

use gtl_application::{
    diffs::{
        compute_commit_patch::{self, ComputeCommitPatch},
        util::assemble,
    },
    ports::GitClient,
    testing::FixedUserSettingsStore,
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::diffs::ExcludedExtensions;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

#[test]
fn assemble_excludes_extensions_at_the_git_level() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@t"]);
    git(d, &["config", "user.name", "t"]);
    std::fs::write(d.join("base.txt"), "base\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "base"]);
    git(d, &["branch", "-M", "main"]);
    git(d, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(d.join("code.rs"), "fn work() {}\n").unwrap();
    std::fs::write(d.join("docs plan.MD"), "l1\nl2\nl3\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "feat: work"]);

    let data = assemble(
        &HybridGitClient,
        d,
        "main...HEAD",
        "main..HEAD",
        &ExcludedExtensions::new(["md"]),
    )
    .unwrap();

    let paths: Vec<&str> = data.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["code.rs"], "git must not emit the excluded file");
    assert_eq!(
        data.hidden_paths,
        ["docs plan.MD"],
        "hidden paths come from the name-only pass, case-insensitively"
    );
    let total_added: u32 = data.files.iter().map(|f| f.added).sum();
    assert_eq!(
        total_added, 1,
        "excluded lines contribute nothing to the totals"
    );
}

#[test]
fn commit_patch_matches_root_and_first_parent_git_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@t"]);
    git(d, &["config", "user.name", "t"]);
    std::fs::write(d.join("f.txt"), "root\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "root"]);

    let source = HybridGitClient;
    let root = source
        .log_commits(d, "HEAD^!")
        .unwrap()
        .into_iter()
        .next()
        .expect("root commit");
    let root_patch = compute_commit_patch::execute(
        ComputeCommitPatch {
            repo_root: d.into(),
            commit: root,
        },
        &FixedUserSettingsStore::default(),
        &source,
    )
    .expect("root patch");
    assert_eq!(
        (root_patch.files[0].added, root_patch.files[0].removed),
        (1, 0)
    );

    std::fs::write(d.join("f.txt"), "root\nselected\n").unwrap();
    git(d, &["commit", "-qam", "selected"]);
    let selected = source
        .log_commits(d, "HEAD^!")
        .unwrap()
        .into_iter()
        .next()
        .expect("selected commit");
    let selected_patch = compute_commit_patch::execute(
        ComputeCommitPatch {
            repo_root: d.into(),
            commit: selected,
        },
        &FixedUserSettingsStore::default(),
        &source,
    )
    .expect("first-parent patch");

    assert_eq!(
        (
            selected_patch.files[0].added,
            selected_patch.files[0].removed
        ),
        (1, 0)
    );
    assert!(
        selected_patch.files[0]
            .lines
            .iter()
            .any(|line| line == "+selected")
    );
}
