//! Behavior tests: run the built binary and assert on help/usage text and exit codes.
//! Command behavior over real git repos lives in `tests/e2e.rs`; these pin the clap-owned
//! surface (subcommand list, flags, and the 0/1/2 exit-code contract).

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
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
        "merge-diff",
        "squash-local",
        "up",
        "tag",
        "wk",
        "status",
        "push-all",
        "pull-all",
        "commit-all",
    ] {
        assert = assert.stdout(contains(sub));
    }
}

#[test]
fn top_level_help_omits_legacy_diff_subrepos_command() {
    git_tools()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("diff-subrepos").not());
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
fn diff_help_documents_the_last_flag() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("--last"))
        .stdout(contains("last N commits"));
}

#[test]
fn diff_help_documents_the_merge_flag() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("-m, --merge"))
        .stdout(contains("BASE...HEAD"));
}

#[test]
fn diff_help_lists_the_subrepos_subcommand() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("subrepos"));
}

#[test]
fn diff_subrepos_help_documents_the_last_flag() {
    git_tools()
        .args(["diff", "subrepos", "--help"])
        .assert()
        .success()
        .stdout(contains("--last"))
        .stdout(contains("last N commits"));
}

#[test]
fn diff_help_documents_the_unpushed_flag() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("--unpushed"))
        .stdout(contains("unpushed"));
}

#[test]
fn diff_help_documents_the_all_flag_and_managed_overrides() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("--all"))
        .stdout(contains("--repos-file"))
        .stdout(contains("--home-dir"));
}

#[test]
fn diff_last_zero_is_usage_error_exit_2() {
    git_tools().args(["diff", "-l", "0"]).assert().code(2);
}

#[test]
fn diff_last_with_target_is_usage_error_exit_2() {
    git_tools()
        .args(["diff", "abc123", "-l", "2"])
        .assert()
        .code(2);
}

#[test]
fn legacy_diff_subrepos_command_is_usage_error_exit_2() {
    git_tools()
        .args(["diff-subrepos", "--repo", "r"])
        .assert()
        .code(2)
        .stderr(contains("unrecognized subcommand"));
}

#[test]
fn squash_local_without_message_is_usage_error_exit_2() {
    git_tools()
        .args(["squash-local", "--repo", "r"])
        .assert()
        .code(2);
}

#[test]
fn up_help_documents_the_yes_flag() {
    git_tools()
        .args(["up", "--help"])
        .assert()
        .success()
        .stdout(contains("--yes"))
        .stdout(contains("push"));
}

#[test]
fn sync_is_no_longer_a_public_subcommand() {
    git_tools()
        .arg("sync")
        .assert()
        .code(2)
        .stderr(contains("unrecognized subcommand"));
}

#[test]
fn up_without_message_is_usage_error_exit_2() {
    git_tools().arg("up").assert().code(2);
}

#[test]
fn up_empty_message_is_usage_error_exit_2() {
    // Guarded before any git runs, so this is safe to assert from the crate dir.
    git_tools()
        .args(["up", ""])
        .assert()
        .code(2)
        .stderr(contains("non-empty commit message"));
}

#[test]
fn tag_help_documents_modes() {
    git_tools()
        .args(["tag", "--help"])
        .assert()
        .success()
        .stdout(contains("--commits"))
        .stdout(contains("add"))
        .stdout(contains("ls"))
        .stdout(contains("up"));
}

#[test]
fn tag_up_create_form_requires_message() {
    git_tools()
        .args(["tag", "up", "v1.2.0"])
        .assert()
        .code(2)
        .stderr(contains("requires both <tag> and <message>"));
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

#[test]
fn status_help_lists_managed_repo_flags() {
    git_tools()
        .args(["status", "--help"])
        .assert()
        .success()
        .stdout(contains("--repos-file"))
        .stdout(contains("--home-dir"))
        .stdout(contains("--json"))
        .stdout(contains("--color"));
}

#[test]
fn ls_alias_routes_to_status_help() {
    git_tools()
        .args(["ls", "--help"])
        .assert()
        .success()
        .stdout(contains("--repos-file"))
        .stdout(contains("--json"));
}

#[test]
fn wk_help_documents_worktree_commands() {
    git_tools()
        .args(["wk", "--help"])
        .assert()
        .success()
        .stdout(contains("base"))
        .stdout(contains("ls"));
}
