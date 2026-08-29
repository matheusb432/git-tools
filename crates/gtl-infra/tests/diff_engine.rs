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
    utils::FixedUserSettingsStore,
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::{
    diffs::DiffExclusions,
    git::GitRange,
    paths::{ProjectName, RepositoryRelativePath, RepositoryRoot},
    settings::{PushAllExclusions, UserSettings},
    viewer::RenderOptions,
};

fn repository_root(path: &Path) -> RepositoryRoot {
    RepositoryRoot::try_new(path.to_path_buf()).unwrap()
}

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
        .unwrap()
        .to_owned();
    let settings = UserSettings::new(
        None,
        RenderOptions::DEFAULT,
        true,
        DiffExclusions::new(
            [(ProjectName::try_new(project).unwrap(), vec!["md"])],
            Some(vec!["txt"]),
        ),
        PushAllExclusions::default(),
    );
    let response = compute_diff::execute(
        ComputeDiff {
            repo_root: repository_root(d),
            target: DiffTarget::Merge {
                base: gtl_models::git::GitRevision::try_new("main").unwrap(),
                pinned: None,
            },
        },
        &FixedUserSettingsStore::new(settings),
        &HybridGitClient,
    )
    .unwrap();

    let paths: Vec<&Path> = response
        .view
        .files
        .iter()
        .map(|file| file.path.as_path())
        .collect();
    assert_eq!(
        paths,
        [Path::new("code.rs")],
        "git must not emit the excluded file"
    );
    assert_eq!(
        response.view.exclusions.unwrap().hidden_paths,
        [RepositoryRelativePath::try_new("docs plan.MD".into()).unwrap()],
        "hidden paths come from the name-only pass, case-insensitively"
    );
    let total_added = response.view.files.iter().fold(
        gtl_models::diffs::DiffLineCount::default(),
        |total, file| total.saturating_add(file.added),
    );
    assert_eq!(
        total_added.value(),
        1,
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
    let repo_root = repository_root(d);
    let selected_commit = GitRange::try_new("HEAD^!").unwrap();
    let root = source
        .log_commits(&repo_root, &selected_commit)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let root_patch = compute_commit_patch::execute(
        ComputeCommitPatch {
            repo_root: repo_root.clone(),
            commit: root,
        },
        &FixedUserSettingsStore::default(),
        &source,
    )
    .unwrap();
    assert_eq!(
        (
            root_patch.files[0].added.value(),
            root_patch.files[0].removed.value()
        ),
        (1, 0)
    );

    std::fs::write(d.join("f.txt"), "root\nselected\n").unwrap();
    git(d, &["commit", "-qam", "selected"]);
    let selected = source
        .log_commits(&repo_root, &selected_commit)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let selected_patch = compute_commit_patch::execute(
        ComputeCommitPatch {
            repo_root,
            commit: selected,
        },
        &FixedUserSettingsStore::default(),
        &source,
    )
    .unwrap();

    assert_eq!(
        (
            selected_patch.files[0].added.value(),
            selected_patch.files[0].removed.value()
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
