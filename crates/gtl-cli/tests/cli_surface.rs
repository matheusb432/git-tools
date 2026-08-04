#![cfg(test)]

//! Surface tests for the grouped `diff` verb (Task 1.5): `merge-diff`/`squash-preview`
//! became `diff merge`/`diff squash`, and the legacy top-level spellings still work
//! through the `preprocess::normalize` argv shim. Real git behavior for these paths is
//! covered by `tests/e2e.rs`; this file pins the clap-owned surface only.

use assert_cmd::Command;
use predicates::{prelude::PredicateBooleanExt, str::contains};

fn git_tools() -> Command {
    Command::cargo_bin("git-tools").unwrap()
}

#[test]
fn diff_merge_is_a_nested_subcommand_with_repo_and_base() {
    git_tools()
        .args(["diff", "merge", "--help"])
        .assert()
        .success()
        .stdout(contains("--repo"))
        .stdout(contains("--base"))
        .stdout(contains("default: main"));
}

#[test]
fn diff_squash_is_a_nested_subcommand_with_repo() {
    git_tools()
        .args(["diff", "squash", "--help"])
        .assert()
        .success()
        .stdout(contains("--repo"));
}

#[test]
fn diff_merge_and_squash_are_not_top_level_commands() {
    git_tools()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("\n  merge-diff ").not())
        .stdout(contains("\n  squash-preview ").not());
}

#[test]
fn diff_help_lists_the_nested_merge_and_squash_subcommands() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("merge"))
        .stdout(contains("squash"));
}

#[test]
fn a_diff_target_flag_conflicts_with_a_nested_subcommand() {
    // `args_conflicts_with_subcommands = true`: DiffTargetArgs's own flags and a
    // `DiffSub` nested subcommand are mutually exclusive.
    git_tools()
        .args(["diff", "-r", "merge", "--repo", "r"])
        .assert()
        .code(2)
        .stderr(contains("error"));
}

#[test]
fn diff_merge_rejects_an_unknown_flag() {
    // MergeArgs has no `--monorepo` of its own (the field was dropped); only the
    // legacy top-level spelling accepts (and strips) it via the shim.
    git_tools()
        .args(["diff", "merge", "--repo", "r", "--monorepo", "m"])
        .assert()
        .code(2)
        .stderr(contains("error"));
}

#[test]
fn legacy_squash_preview_with_monorepo_still_parses_via_the_shim() {
    // The shim rewrites `squash-preview --repo r --monorepo m` to
    // `diff squash --repo r`, stripping the now-dropped `monorepo` pair, so this
    // still reaches `--help` successfully instead of erroring on an unknown flag.
    git_tools()
        .args(["squash-preview", "--repo", "r", "--monorepo", "m", "--help"])
        .assert()
        .success()
        .stdout(contains("--repo"));
}

#[test]
fn legacy_merge_diff_with_monorepo_still_parses_via_the_shim() {
    git_tools()
        .args([
            "merge-diff",
            "--repo",
            "r",
            "--monorepo",
            "m",
            "--base",
            "trunk",
            "--help",
        ])
        .assert()
        .success()
        .stdout(contains("--base"));
}
