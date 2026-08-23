use std::{
    env,
    os::unix::fs::PermissionsExt as _,
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
        let temporary = tempfile::tempdir().context("temporary push fixture")?;
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

    fn run(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_git-tools"));
        command.args(arguments).current_dir(&self.repository);
        command
    }

    fn config_path(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }

    fn configure_managed_projects(&self) -> Result<()> {
        let bin = self.temporary.path().join("bin");
        std::fs::create_dir_all(&bin).context("create fixture binary directory")?;
        let sample_project = bin.join("sample_project");
        std::fs::write(
            &sample_project,
            r#"#!/bin/sh
printf '%s' '[{"id":"REP","title":"repo","mux_session_name":"rep","source":{"kind":"directory","value":"~/repo"},"git_remote":null,"is_paused":false,"affiliation":"personal","color":null,"groups":[]}]'
"#,
        )
        .context("write fixture sample_project executable")?;
        std::fs::set_permissions(&sample_project, std::fs::Permissions::from_mode(0o755))
            .context("make fixture sample_project executable")?;

        let current_path = env::var_os("PATH").context("PATH is configured")?;
        let path = env::join_paths(std::iter::once(bin).chain(env::split_paths(&current_path)))
            .context("compose fixture PATH")?;
        // This integration-test binary has one test, so no other test can observe these values.
        unsafe {
            env::set_var("HOME", self.temporary.path());
            env::set_var("PATH", path);
        }
        Ok(())
    }

    fn commits_unpushed_count(&self) -> Result<usize> {
        self.git(&["rev-list", "--count", "@{u}..HEAD"])?
            .parse()
            .context("parse unpushed commit count")
    }
}

fn managed_report(fixture: &PushFixture, config: &Path) -> Result<serde_json::Value> {
    let output = fixture
        .run(&["push", "--all", "--dry", "--json"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).context("parse managed push report")
}

#[test]
fn push_modes_apply_exclusions_only_to_push_all() -> Result<()> {
    let fixture = PushFixture::new()?;
    fixture.configure_managed_projects()?;
    let config = fixture.config_path("config.toml");
    let _server = common::ServerHarness::start(Some(&config))?;

    fixture
        .run(&["push"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .code(2)
        .stdout(contains("review before pushing"))
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
        .stdout(contains("push: pushed 1 commit(s)"))
        .stderr(contains("pass --yes").not());
    assert_eq!(fixture.commits_unpushed_count()?, 0);

    let recursive = PushFixture::new()?;
    recursive
        .run(&["push", "--recursive", "--yes"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .stdout(contains("1 repos: 1 pushed"));
    assert_eq!(recursive.commits_unpushed_count()?, 0);
    Ok(())
}
