//! End-to-end tests for recursive `gtl push -r`: build a real tree of repos (the root plus
//! nested ones under a subfolder), each wired to a bare upstream, run the built binary, and
//! assert exit code, stdout, and that each remote advanced. Local-only — bare remotes on
//! disk, no network.

use std::{path::Path, process::Command as Git};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// Run a git command in `dir`, asserting success.
fn git(dir: &Path, args: &[&str]) {
    let out = Git::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `git rev-parse <rev>` (trimmed) in `dir`.
fn rev(dir: &Path, r: &str) -> String {
    let out = Git::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", r])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "rev-parse {r} failed in {}",
        dir.display()
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn config_identity(repo: &Path) {
    git(repo, &["config", "user.name", "E2E Bot"]);
    git(repo, &["config", "user.email", "e2e@example.invalid"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
}

/// Create a repo at `path` on `main`, wired to a fresh bare upstream under `remotes/`, with
/// `main` tracking `origin/main`. Returns the bare remote's path. With `ahead`, the repo
/// gets one extra commit not yet on the remote (so `push -r` has something to push).
fn repo_with_upstream(remotes: &Path, path: &Path, key: &str, ahead: bool) -> std::path::PathBuf {
    let remote = remotes.join(format!("{key}.git"));
    git(remotes, &["init", "-q", "--bare", remote.to_str().unwrap()]);

    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "-b", "main"]);
    config_identity(path);
    git(path, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git(
        path,
        &["commit", "-q", "--allow-empty", "-m", "chore: base"],
    );
    git(path, &["push", "-q", "-u", "origin", "main"]);
    if ahead {
        git(
            path,
            &["commit", "-q", "--allow-empty", "-m", "feat: ahead"],
        );
    }
    remote
}

/// The built `git-tools` binary, run with cwd inside `repo`.
fn gtl(repo: &Path) -> Command {
    let mut cmd = Command::cargo_bin("git-tools").unwrap();
    cmd.current_dir(repo);
    cmd
}

#[test]
fn push_recursive_yes_pushes_root_and_nested_repos() {
    let tmp = TempDir::new().unwrap();
    let remotes = tmp.path().join("remotes");
    std::fs::create_dir_all(&remotes).unwrap();
    let work = tmp.path().join("work");

    // The root is itself a repo, with nested repos under `bar/` (the user's foo + foo/bar).
    let root_remote = repo_with_upstream(&remotes, &work, "root", true);
    let api = work.join("bar/api");
    let api_remote = repo_with_upstream(&remotes, &api, "api", true);
    let web = work.join("bar/web");
    let web_remote = repo_with_upstream(&remotes, &web, "web", false); // already up to date

    gtl(&work)
        .args(["push", "-r", "-y"])
        .assert()
        .success()
        .stdout(contains("pushed 3 repo(s)"))
        // The already-synced repo is spotted at plan time (local `@{u}..HEAD` == 0) and
        // reported as synced in the confirmation — never pushed over the network.
        .stdout(contains("already synced"))
        .stdout(contains("already up to date"));

    // Every remote now matches its repo's HEAD — the unpushed commits landed.
    assert_eq!(rev(&root_remote, "main"), rev(&work, "HEAD"), "root pushed");
    assert_eq!(rev(&api_remote, "main"), rev(&api, "HEAD"), "api pushed");
    assert_eq!(rev(&web_remote, "main"), rev(&web, "HEAD"), "web current");
}

#[test]
fn push_recursive_lists_repos_in_confirmation_before_pushing() {
    let tmp = TempDir::new().unwrap();
    let remotes = tmp.path().join("remotes");
    std::fs::create_dir_all(&remotes).unwrap();
    let work = tmp.path().join("work");
    repo_with_upstream(&remotes, &work, "root", true);
    repo_with_upstream(&remotes, &work.join("bar/api"), "api", true);

    gtl(&work)
        .args(["push", "-r", "-y"])
        .assert()
        .success()
        // The review block names the destination of each discovered repo.
        .stdout(contains("push 2 repo(s) under"))
        .stdout(contains("(main → origin)"));
}

#[test]
fn push_recursive_skips_a_repo_without_an_upstream_without_failing() {
    let tmp = TempDir::new().unwrap();
    let remotes = tmp.path().join("remotes");
    std::fs::create_dir_all(&remotes).unwrap();
    let work = tmp.path().join("work");
    repo_with_upstream(&remotes, &work, "root", true);

    // A nested repo with commits but no remote configured — must be skipped, not fail.
    let loose = work.join("bar/loose");
    std::fs::create_dir_all(&loose).unwrap();
    git(&loose, &["init", "-q", "-b", "main"]);
    config_identity(&loose);
    git(
        &loose,
        &["commit", "-q", "--allow-empty", "-m", "local only"],
    );

    gtl(&work)
        .args(["push", "-r", "-y"])
        .assert()
        .success()
        .stdout(contains("skipped — no upstream tracking branch"));
}

#[test]
fn push_recursive_non_interactive_without_yes_refuses() {
    let tmp = TempDir::new().unwrap();
    let remotes = tmp.path().join("remotes");
    std::fs::create_dir_all(&remotes).unwrap();
    let work = tmp.path().join("work");
    let root_remote = repo_with_upstream(&remotes, &work, "root", true);
    let before = rev(&root_remote, "main");

    gtl(&work)
        .args(["push", "-r"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("push -r: non-interactive shell; pass --yes"));

    assert_eq!(
        rev(&root_remote, "main"),
        before,
        "nothing pushed on refusal"
    );
}
