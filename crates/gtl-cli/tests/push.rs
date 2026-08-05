use std::{path::PathBuf, process};

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;
use predicates::{prelude::PredicateBooleanExt as _, str::contains};
use tempfile::TempDir;

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

    fn commits_unpushed_count(&self) -> Result<usize> {
        self.git(&["rev-list", "--count", "@{u}..HEAD"])?
            .parse()
            .context("parse unpushed commit count")
    }
}

#[test]
fn push_confirmation_can_be_rejected_then_disabled() -> Result<()> {
    let fixture = PushFixture::new()?;
    let config = fixture.config_path("config.toml");

    fixture
        .run(&["push"])
        .env("GIT_TOOLS_CONFIG", &config)
        .assert()
        .code(2)
        .stdout(contains("review before pushing"))
        .stderr(contains("pass --yes"));
    assert_eq!(fixture.commits_unpushed_count()?, 1);

    std::fs::write(&config, "[push]\nconfirm = false\n").context("write fixture config")?;
    fixture
        .run(&["p"])
        .env("GIT_TOOLS_CONFIG", config)
        .assert()
        .success()
        .stdout(contains("review before pushing").not())
        .stdout(contains("push: pushed 1 commit(s)"))
        .stderr(contains("pass --yes").not());
    assert_eq!(fixture.commits_unpushed_count()?, 0);
    Ok(())
}
