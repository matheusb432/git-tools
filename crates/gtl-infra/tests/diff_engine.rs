#![cfg(test)]

//! Real-git integration tests for application diff slices driven through public
//! interactor boundaries and the `HybridGitClient` adapter. The pure parsing paths
//! are unit-tested in `application`; here we prove the slices against actual Git
//! output on fixture repositories.

use std::{path::Path, process::Command};

use gtl_application::{
    diffs::{
        DiffTarget,
        compute_commit_patch::{self, ComputeCommitPatch},
        compute_diff::{self, ComputeDiff},
    },
    ports::GitClient,
    testing::FixedUserSettingsStore,
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::{diffs::DiffExclusions, settings::UserSettings, viewer::RenderOptions};

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

    let project = d
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temporary repository has a UTF-8 project name")
        .to_owned();
    let settings = UserSettings::new(
        None,
        RenderOptions::DEFAULT,
        true,
        DiffExclusions::new(
            [
                (project, vec!["md"]),
                (DiffExclusions::DEFAULT_KEY.to_owned(), vec!["txt"]),
            ],
            None,
        ),
    );
    let response = compute_diff::execute(
        ComputeDiff {
            cwd: d.to_path_buf(),
            target: DiffTarget::Merge {
                base: "main".into(),
                pinned: None,
            },
        },
        &FixedUserSettingsStore::new(settings),
        &HybridGitClient,
    )
    .expect("compute diff succeeds");

    let paths: Vec<&str> = response
        .view
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, ["code.rs"], "git must not emit the excluded file");
    assert_eq!(
        response
            .view
            .exclusions
            .expect("excluded path metadata")
            .hidden_paths,
        ["docs plan.MD"],
        "hidden paths come from the name-only pass, case-insensitively"
    );
    let total_added: u32 = response.view.files.iter().map(|file| file.added).sum();
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
