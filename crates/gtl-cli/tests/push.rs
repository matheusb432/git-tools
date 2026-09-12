use std::{
    path::{Path, PathBuf},
    process,
};

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;
use predicates::{prelude::PredicateBooleanExt as _, str::contains};
use tempfile::TempDir;

mod common;

struct PushFixture {
    temporary: TempDir,
    repository: PathBuf,
}

impl PushFixture {
    fn new() -> Result<Self> {
        let home = std::env::var_os("HOME").context("fixture home")?;
        let temporary = tempfile::tempdir_in(home).context("temporary push fixture")?;
        let repository = temporary.path().join("repo");
        let remote = temporary.path().join("origin.git");
        std::fs::create_dir_all(&repository).context("create repository directory")?;

        let fixture = Self {
            temporary,
            repository,
        };
        fixture.git(&["init", "-q", "-b", "main"])?;
        fixture.git(&["config", "user.name", "E2E Bot"])?;
        fixture.git(&["config", "user.email", "e2e@example.invalid"])?;
        fixture.git(&["config", "commit.gpgsign", "false"])?;
        fixture.git(&["config", "core.autocrlf", "false"])?;
        fixture.commit("base\n", "chore: base")?;

        let output = process::Command::new("git")
            .args(["init", "--bare", "-q"])
            .arg(&remote)
            .output()
            .context("initialize bare remote")?;
        ensure!(
            output.status.success(),
            "bare remote initialization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let remote = remote.to_string_lossy();
        fixture.git(&["remote", "add", "origin", &remote])?;
        fixture.git(&["push", "-q", "-u", "origin", "main"])?;
        fixture.commit("base\nlocal\n", "feat: local work")?;
        Ok(fixture)
    }

    fn git(&self, arguments: &[&str]) -> Result<String> {
        let output = process::Command::new("git")
            .arg("-C")
            .arg(&self.repository)
            .args(arguments)
            .output()
            .context("run Git")?;
        ensure!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)
            .context("Git stdout is UTF-8")?
            .trim()
            .to_string())
    }

    fn commit(&self, contents: &str, message: &str) -> Result<()> {
        std::fs::write(self.repository.join("work.txt"), contents).context("write fixture file")?;
        self.git(&["add", "work.txt"])?;
        let output = process::Command::new("git")
            .arg("-C")
            .arg(&self.repository)
            .args(["commit", "-q", "-m", message])
            .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00Z")
            .output()
            .context("commit fixture change")?;
        ensure!(
            output.status.success(),
            "commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn write_untracked_file(&self, path: &str, contents: &str) -> Result<()> {
        let path = self.repository.join(path);
        let parent = path
            .parent()
            .context("untracked fixture file has a parent")?;
        std::fs::create_dir_all(parent).context("create untracked fixture directory")?;
        std::fs::write(path, contents).context("write untracked fixture file")
    }

    fn run(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_git-tools"));
        command.args(arguments).current_dir(&self.repository);
        command
    }

    fn config_path(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }

    fn commits_unpushed_count(&self) -> Result<usize> {
        self.git(&["rev-list", "--count", "@{u}..HEAD"])?
            .parse()
            .context("parse unpushed commit count")
    }
}

fn managed_report(fixture: &PushFixture, config: &Path) -> Result<serde_json::Value> {
    let output = fixture
        .run(&["project", "push", "--all", "--dry", "--json"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).context("parse managed push report")
}

#[test]
fn push_modes_apply_exclusions_and_commit_nested_untracked_files() -> Result<()> {
    let fixture = PushFixture::new()?;
    let config = fixture.config_path("config.toml");
    let _server = common::ServerHarness::start(Some(&config), Some(&fixture.repository))?;

    fixture
        .run(&["push"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .code(2)
        .stdout("")
        .stderr(contains("pass --yes"));
    assert_eq!(fixture.commits_unpushed_count()?, 1);

    let report = managed_report(&fixture, &config)?;
    assert_eq!(report["Selected"][0]["Name"], "repo");
    assert_eq!(report["Selected"][0]["Status"], "would-push");
    assert_eq!(report["Excluded"], serde_json::json!([]));

    std::fs::write(
        &config,
        r#"
[[projects]]
name = "unknown"
excluded_from_push_all = true
"#,
    )
    .context("write unknown exclusion config")?;
    let report = managed_report(&fixture, &config)?;
    assert_eq!(report["Selected"][0]["Name"], "repo");
    assert_eq!(report["Excluded"], serde_json::json!([]));

    std::fs::write(
        &config,
        r#"
[push]
confirm = false

[[projects]]
name = "repo"
excluded_from_push_all = true
"#,
    )
    .context("write fixture config")?;
    let report = managed_report(&fixture, &config)?;
    assert_eq!(report["Selected"], serde_json::json!([]));
    assert_eq!(report["Excluded"], serde_json::json!(["repo"]));
    assert_eq!(fixture.commits_unpushed_count()?, 1);

    fixture
        .run(&["p"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .success()
        .stdout(contains("review before pushing").not())
        .stdout(contains("Pushed 1 commit"))
        .stderr(contains("pass --yes").not());
    assert_eq!(fixture.commits_unpushed_count()?, 0);

    fixture.write_untracked_file("new-work/nested.txt", "new work\n")?;
    fixture
        .run(&["p", "--yes", "feat: add nested work"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .success()
        .stdout(contains("Staged, committed, and pushed"));
    assert_eq!(fixture.git(&["status", "--porcelain"])?, "");
    assert_eq!(
        fixture.git(&["log", "-1", "--format=%s"])?,
        "feat: add nested work"
    );
    assert_eq!(
        fixture.git(&["show", "origin/main:new-work/nested.txt"])?,
        "new work"
    );
    assert_eq!(fixture.commits_unpushed_count()?, 0);

    let recursive = PushFixture::new()?;
    recursive
        .run(&["push", "--recursive", "--yes"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .stdout(contains("1 project: 1 pushed"));
    assert_eq!(recursive.commits_unpushed_count()?, 0);
    Ok(())
}
