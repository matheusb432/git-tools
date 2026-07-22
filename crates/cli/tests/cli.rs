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
        "diff",
        "squash-local",
        "push",
        "pull",
        "commit",
        "tag",
        "wk",
        "status",
        "ls",
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
fn diff_help_documents_the_last_flag() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("--last"))
        .stdout(contains("last N commits"));
}

#[test]
fn diff_help_documents_recursive_scope() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("-r, --recursive"))
        .stdout(contains("nested subrepos"));
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
fn d_alias_routes_to_diff_help() {
    git_tools()
        .args(["d", "--help"])
        .assert()
        .success()
        .stdout(contains("--recursive"))
        .stdout(contains("--all"));
}

#[test]
fn diff_help_documents_set_theme() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("--set-theme"));
}

#[test]
fn diff_set_theme_uses_the_user_settings_operation() {
    let directory = tempfile::tempdir().expect("temp directory");
    let config = directory.path().join("nested").join("config.toml");
    std::fs::create_dir_all(config.parent().expect("config parent")).expect("parent");
    std::fs::write(&config, "layout = \"split\"\n").expect("seed config");

    git_tools()
        .args(["diff", "--set-theme", "hearth"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .success()
        .stdout(contains("hearth"))
        .stdout(contains(config.display().to_string()));

    let raw = std::fs::read_to_string(config).expect("updated config");
    let document = toml::from_str::<toml::Value>(&raw).expect("valid settings TOML");
    assert_eq!(document["theme"].as_str(), Some("hearth"));
    assert_eq!(document["layout"].as_str(), Some("split"));
}

#[test]
fn diff_set_theme_rejects_a_non_string_existing_theme_without_modifying_it() {
    let directory = tempfile::tempdir().expect("temp directory");
    let config = directory.path().join("config.toml");
    let raw = "theme = 7\nlayout = \"split\"\n";
    std::fs::write(&config, raw).expect("seed config");

    git_tools()
        .args(["diff", "--set-theme", "light"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .code(1)
        .stderr(contains("user setting `theme` must be a string"));

    assert_eq!(std::fs::read_to_string(config).unwrap(), raw);
}

#[test]
fn diff_set_theme_rejects_unknown_value_exit_2() {
    git_tools()
        .args(["diff", "--set-theme", "neon"])
        .assert()
        .code(2)
        .stderr(contains("error"));
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
fn diff_help_documents_the_name_flag() {
    git_tools()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(contains("-n, --name"))
        .stdout(contains("history label"));
}

#[test]
fn diff_blank_name_is_usage_error_exit_2() {
    git_tools().args(["diff", "--name", "   "]).assert().code(2);
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
fn squash_local_without_message_is_usage_error_exit_2() {
    git_tools()
        .args(["squash-local", "--repo", "r"])
        .assert()
        .code(2);
}

#[test]
fn push_help_documents_message_and_scope() {
    git_tools()
        .args(["push", "--help"])
        .assert()
        .success()
        .stdout(contains("Commit message"))
        .stdout(contains("--all"))
        .stdout(contains("--recursive"))
        .stdout(contains("--yes"));
}

#[test]
fn p_alias_routes_to_push_help() {
    git_tools()
        .args(["p", "--help"])
        .assert()
        .success()
        .stdout(contains("--recursive"))
        .stdout(contains("--all"));
}

#[test]
fn commit_help_documents_message() {
    git_tools()
        .args(["commit", "--help"])
        .assert()
        .success()
        .stdout(contains("Commit message"))
        .stdout(contains("--all"))
        .stdout(contains("--yes"));
}

#[test]
fn push_empty_message_is_usage_error_exit_2() {
    // Guarded before any git runs, so this is safe to assert from the crate dir.
    git_tools()
        .args(["push", ""])
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
        .stdout(contains("push"));
}

#[test]
fn tag_push_create_form_requires_message() {
    git_tools()
        .args(["tag", "push", "v1.2.0"])
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
fn diff_merge_help_notes_the_default_base() {
    git_tools()
        .args(["diff", "merge", "--help"])
        .assert()
        .success()
        .stdout(contains("--base"))
        .stdout(contains("default: main"));
}

#[test]
fn legacy_merge_diff_invocation_still_works_via_shim() {
    // The migration bridge in `preprocess::normalize` rewrites the old top-level
    // `merge-diff` invocation onto `diff merge --help` before clap parses it.
    git_tools()
        .args(["merge-diff", "--help"])
        .assert()
        .success()
        .stdout(contains("--base"))
        .stdout(contains("default: main"));
}

#[test]
fn legacy_squash_preview_invocation_still_works_via_shim() {
    git_tools()
        .args(["squash-preview", "--help"])
        .assert()
        .success()
        .stdout(contains("--repo"));
}

#[test]
fn managed_verbs_document_all_scope() {
    git_tools()
        .args(["push", "--help"])
        .assert()
        .success()
        .stdout(contains("--all"));
    git_tools()
        .args(["pull", "--help"])
        .assert()
        .success()
        .stdout(contains("--all"));
    git_tools()
        .args(["commit", "--help"])
        .assert()
        .success()
        .stdout(contains("--all"));
}

#[test]
fn bare_pull_is_usage_error_exit_2() {
    git_tools().arg("pull").assert().code(2);
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
fn wk_help_documents_worktree_commands() {
    git_tools()
        .args(["wk", "--help"])
        .assert()
        .success()
        .stdout(contains("base"))
        .stdout(contains("ls"));
}
