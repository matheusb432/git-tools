#![cfg(test)]

use std::{path::PathBuf, process};

use assert_cmd::Command;
use predicates::{prelude::PredicateBooleanExt as _, str::contains};
use tempfile::TempDir;

mod support;
use support::workspace_bin;

struct PushFixture {
    temporary: TempDir,
    repository: PathBuf,
}

impl PushFixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temporary push fixture");
        let repository = temporary.path().join("repo");
        let remote = temporary.path().join("origin.git");
        std::fs::create_dir_all(&repository).expect("repository directory");

        let fixture = Self {
            temporary,
            repository,
        };
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.git(&["config", "user.name", "E2E Bot"]);
        fixture.git(&["config", "user.email", "e2e@example.invalid"]);
        fixture.git(&["config", "commit.gpgsign", "false"]);
        fixture.git(&["config", "core.autocrlf", "false"]);
        fixture.commit("base\n", "chore: base");

        let output = process::Command::new("git")
            .args(["init", "--bare", "-q"])
            .arg(&remote)
            .output()
            .expect("initialize bare remote");
        assert!(
            output.status.success(),
            "bare remote initialization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fixture.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        fixture.git(&["push", "-q", "-u", "origin", "main"]);
        fixture.commit("base\nlocal\n", "feat: local work");
        fixture
    }

    fn git(&self, arguments: &[&str]) -> String {
        let output = process::Command::new("git")
            .arg("-C")
            .arg(&self.repository)
            .args(arguments)
            .output()
            .expect("run Git");
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("Git stdout is UTF-8")
            .trim()
            .to_string()
    }

    fn commit(&self, contents: &str, message: &str) {
        std::fs::write(self.repository.join("work.txt"), contents).expect("write fixture file");
        self.git(&["add", "work.txt"]);
        let output = process::Command::new("git")
            .arg("-C")
            .arg(&self.repository)
            .args(["commit", "-q", "-m", message])
            .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00Z")
            .output()
            .expect("commit fixture change");
        assert!(
            output.status.success(),
            "commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(workspace_bin("git-tools"));
        command.args(arguments).current_dir(&self.repository);
        command
    }

    fn config_path(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }

    fn commits_unpushed_count(&self) -> usize {
        self.git(&["rev-list", "--count", "@{u}..HEAD"])
            .parse()
            .expect("unpushed commit count")
    }
}

fn git_tools() -> Command {
    Command::new(workspace_bin("git-tools"))
}

#[test]
fn plain_push_without_yes_requires_confirmation_by_default() {
    let fixture = PushFixture::new();
    let config = fixture.config_path("missing-config.toml");

    fixture
        .run(&["push"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .code(2)
        .stdout(contains("review before pushing"))
        .stderr(contains("pass --yes"));

    assert_eq!(fixture.commits_unpushed_count(), 1);
}

#[test]
fn plain_push_skips_confirmation_when_configured() {
    let fixture = PushFixture::new();
    let config = fixture.config_path("config.toml");
    std::fs::write(&config, "[push]\nconfirm = false\n").expect("write fixture config");

    fixture
        .run(&["p"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .stdout(contains("review before pushing").not())
        .stdout(contains("push: pushed 1 commit(s)"))
        .stderr(contains("pass --yes").not());

    assert_eq!(fixture.commits_unpushed_count(), 0);
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
