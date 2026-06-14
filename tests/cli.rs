//! Behavior tests: run the built binary and assert on help/usage text and exit codes.
//! Command behavior over real git repos lives in `tests/e2e.rs`; these pin the clap-owned
//! surface (subcommand list, flags, and the 0/1/2 exit-code contract).

use assert_cmd::Command;
use predicates::str::contains;

fn git_tools() -> Command {
    Command::cargo_bin("git-tools").unwrap()
}

#[test]
fn top_level_help_lists_every_subcommand() {
    let mut assert = git_tools().arg("--help").assert().success();
    for sub in [
        "squash-preview",
        "diff",
        "diff-subrepos",
        "merge-diff",
        "squash-local",
        "push-all",
        "pull-all",
        "commit-all",
    ] {
        assert = assert.stdout(contains(sub));
    }
}

#[test]
fn no_args_prints_help_guidance_and_succeeds() {
    // A bare invocation is guidance, not data: clap writes help to stderr and we exit 0.
    git_tools()
        .assert()
        .success()
        .stderr(contains("Usage: git-tools"));
}

#[test]
fn version_flag_succeeds_and_names_the_tool() {
    git_tools()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("git-tools"));
}

#[test]
fn unknown_command_is_usage_error_exit_2() {
    git_tools()
        .arg("definitely-not-a-command")
        .assert()
        .code(2)
        .stderr(contains("error"));
}

#[test]
fn lean_diff_rejects_explicit_flags_exit_2() {
    git_tools().args(["diff", "--repo", "r"]).assert().code(2);
}

#[test]
fn diff_subrepos_without_required_flags_is_usage_error_exit_2() {
    // clap enforces the required --repo/--monorepo pair (only --repo given here).
    git_tools()
        .args(["diff-subrepos", "--repo", "r"])
        .assert()
        .code(2);
}

#[test]
fn squash_local_without_message_is_usage_error_exit_2() {
    git_tools()
        .args(["squash-local", "--repo", "r"])
        .assert()
        .code(2);
}

#[test]
fn squash_local_help_lists_repo_and_dry() {
    git_tools()
        .args(["squash-local", "--help"])
        .assert()
        .success()
        .stdout(contains("--repo"))
        .stdout(contains("--dry"));
}

#[test]
fn merge_diff_help_notes_the_default_base() {
    git_tools()
        .args(["merge-diff", "--help"])
        .assert()
        .success()
        .stdout(contains("--base"))
        .stdout(contains("default: main"));
}

#[test]
fn commit_all_help_lists_message_for_all() {
    git_tools()
        .args(["commit-all", "--help"])
        .assert()
        .success()
        .stdout(contains("--message-for-all"));
}
