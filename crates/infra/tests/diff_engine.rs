//! Real-git integration tests for the diff engine (`application::diffs`) driven
//! through the `GitDiffSource` adapter. The pure parsing paths are unit-tested in
//! `application`; here we prove the engine against actual `git log`/`git blame`
//! output on fixture repos.

use std::{path::Path, process::Command};

use application::diffs::{
    attribution::{NewSide, attribute},
    util::assemble,
};
use domain::diffs::{FileDiff, LineOwners};
use infra::diff_source::GitDiffSource;

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

fn short_head(dir: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--short=9", "HEAD"])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn assemble_attaches_brought_in_members_to_a_merge() {
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
    std::fs::write(d.join("a.txt"), "a\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "feat a"]);
    git(d, &["checkout", "-q", "-b", "sub"]);
    std::fs::write(d.join("b.txt"), "b\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "sub b"]);
    git(d, &["checkout", "-q", "feature"]);
    git(
        d,
        &["merge", "-q", "--no-ff", "sub", "-m", "Merge branch 'sub'"],
    );

    let data = assemble(
        &GitDiffSource,
        d,
        &["diff".to_string(), "main...HEAD".to_string()],
        "main...HEAD",
        "main..HEAD",
    )
    .unwrap();

    let merge = data
        .commits
        .iter()
        .find(|c| c.is_merge())
        .expect("a merge commit");
    let sub_b = data.commits.iter().find(|c| c.subject == "sub b").unwrap();
    let feat_a = data.commits.iter().find(|c| c.subject == "feat a").unwrap();

    assert!(
        merge.members.contains(&sub_b.sha),
        "merge lists its brought-in commit"
    );
    assert!(
        !merge.members.contains(&feat_a.sha),
        "first-parent commit is not a member"
    );
    assert!(
        feat_a.members.is_empty(),
        "a non-merge commit has no members"
    );
}

#[test]
fn attribute_owns_added_and_deleted_lines_by_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@t"]);
    git(d, &["config", "user.name", "t"]);
    std::fs::write(d.join("f.txt"), "L1\nL2\nL3\nL4\nL5\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "base"]);
    let base = short_head(d);
    // c1 inserts ADD at new line 3
    std::fs::write(d.join("f.txt"), "L1\nL2\nADD\nL3\nL4\nL5\n").unwrap();
    git(d, &["commit", "-qam", "c1"]);
    let c1 = short_head(d);
    // c2 deletes L4 (base line 4)
    std::fs::write(d.join("f.txt"), "L1\nL2\nADD\nL3\nL5\n").unwrap();
    git(d, &["commit", "-qam", "c2"]);
    let c2 = short_head(d);

    let mut files = vec![FileDiff {
        path: "f.txt".to_string(),
        added: 0,
        removed: 0,
        lines: Vec::new(),
        full_lines: None,
        commits: Vec::new(),
        owners: LineOwners::default(),
    }];
    let in_range = [c1.clone(), c2.clone()].into_iter().collect();
    attribute(
        &GitDiffSource,
        d,
        &base,
        &NewSide::Commit("HEAD".to_string()),
        &in_range,
        &mut files,
    );

    assert_eq!(files[0].owners.added.get(&3), Some(&c1)); // ADD at new line 3 -> c1
    assert_eq!(files[0].owners.deleted.get(&4), Some(&c2)); // base line 4 (L4) -> c2
}
