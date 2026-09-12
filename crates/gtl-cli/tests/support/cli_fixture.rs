use std::{
    path::{Path, PathBuf},
    process,
    time::Duration,
};

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;
use expectrl::Expect as _;

pub struct CliFixture {
    temporary: tempfile::TempDir,
    repository: PathBuf,
}

#[derive(Clone, Copy)]
pub enum CancelKey {
    No,
    Quit,
    Escape,
}

pub struct BareRemote(PathBuf);

impl BareRemote {
    fn create(path: PathBuf) -> Result<Self> {
        run_git(
            process::Command::new("git")
                .args(["init", "--bare", "-q"])
                .arg(&path),
        )?;
        Ok(Self(path))
    }

    pub fn tags(&self) -> Result<Vec<String>> {
        let tags = run_git(
            process::Command::new("git")
                .arg("-C")
                .arg(&self.0)
                .args(["tag", "--list"]),
        )?;
        Ok(tags.lines().map(str::to_string).collect())
    }
}

impl CliFixture {
    pub fn new() -> Result<Self> {
        let temporary = tempfile::tempdir()?;
        let repository = temporary.path().join("example-project");
        std::fs::create_dir(&repository)?;
        let fixture = Self {
            temporary,
            repository,
        };
        fixture.git(&["init", "-q", "-b", "main"])?;
        for (key, value) in [
            ("user.name", "Example Author"),
            ("user.email", "author@example.invalid"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
        ] {
            fixture.git(&["config", key, value])?;
        }
        fixture.git(&["commit", "-q", "--allow-empty", "-m", "Initial commit"])?;
        fixture.git(&["tag", "v1.0.0"])?;
        let remote = BareRemote::create(fixture.temporary.path().join("origin.git"))?;
        run_git(
            fixture
                .git_command()
                .args(["remote", "add", "origin"])
                .arg(remote.0),
        )?;
        fixture.git(&["push", "-q", "-u", "origin", "main"])?;
        Ok(fixture)
    }

    pub fn config_path(&self) -> PathBuf {
        self.temporary.path().join("config.toml")
    }

    pub fn write_file(&self, path: &str, contents: &str) -> Result<()> {
        std::fs::write(self.repository.join(path), contents).context("write fixture file")
    }

    pub fn head(&self) -> Result<String> {
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn working_tree(&self) -> Result<String> {
        self.git(&["status", "--porcelain"])
    }

    pub fn unpushed_commits(&self) -> Result<usize> {
        self.git(&["rev-list", "--count", "@{u}..HEAD"])?
            .parse()
            .context("parse unpushed commit count")
    }

    pub fn tags(&self) -> Result<Vec<String>> {
        Ok(self
            .git(&["tag", "--list"])?
            .lines()
            .map(str::to_string)
            .collect())
    }

    pub fn last_commit_message(&self) -> Result<String> {
        self.git(&["log", "-1", "--format=%s"])
    }

    pub fn use_push_remote(&self, name: &str) -> Result<BareRemote> {
        let remote = BareRemote::create(self.temporary.path().join(name))?;
        self.set_push_url(&remote.0)?;
        Ok(remote)
    }

    pub fn add_push_remote(&self, name: &str) -> Result<BareRemote> {
        let remote = BareRemote::create(self.temporary.path().join(name))?;
        run_git(
            self.git_command()
                .args(["config", "--add", "remote.origin.pushurl"])
                .arg(&remote.0),
        )?;
        Ok(remote)
    }

    pub fn make_push_remote_unavailable(&self) -> Result<()> {
        self.set_push_url(&self.temporary.path().join("unavailable.git"))
    }

    fn set_push_url(&self, path: &Path) -> Result<()> {
        run_git(
            self.git_command()
                .args(["config", "--replace-all", "remote.origin.pushurl"])
                .arg(path),
        )?;
        Ok(())
    }

    #[track_caller]
    pub fn succeeds(&self, arguments: &[&str], stdout: &str) {
        self.run(arguments)
            .assert()
            .success()
            .stdout(stdout.to_owned())
            .stderr("");
    }

    #[track_caller]
    pub fn output(&self, arguments: &[&str]) -> Result<String> {
        let result = self.run(arguments).assert().success().stderr("");
        String::from_utf8(result.get_output().stdout.clone()).context("CLI output is UTF-8")
    }

    #[track_caller]
    pub fn fails(&self, arguments: &[&str], exit_code: i32, stderr: &[&str]) {
        let result = self.run(arguments).assert().code(exit_code).stdout("");
        assert_contains(
            &String::from_utf8_lossy(&result.get_output().stderr),
            stderr,
        );
    }

    #[track_caller]
    pub fn requires_confirmation(&self, arguments: &[&str]) {
        self.fails(arguments, 2, &["pass --yes"]);
    }

    pub fn cancel(&self, arguments: &[&str], key: CancelKey) -> Result<()> {
        let answer = match key {
            CancelKey::No => "n",
            CancelKey::Quit => "q",
            CancelKey::Escape => "\u{1b}",
        };
        self.answer(arguments, answer, "cancelled")
    }

    pub fn accept_default(&self, arguments: &[&str], result: &str) -> Result<()> {
        self.answer(arguments, "\n", result)
    }

    fn answer(&self, arguments: &[&str], answer: &str, expected: &str) -> Result<()> {
        let mut session = expectrl::Session::spawn(self.command(arguments))?;
        session.set_expect_timeout(Some(Duration::from_secs(15)));
        session.expect("(y/n)")?;
        session.send(answer)?;
        session.expect(expected)?;
        session.expect(expectrl::Eof)?;
        ensure!(
            matches!(
                session.get_process().wait()?,
                expectrl::process::unix::WaitStatus::Exited(_, 0)
            ),
            "CLI {arguments:?} did not exit successfully"
        );
        Ok(())
    }

    fn command(&self, arguments: &[&str]) -> process::Command {
        let mut command = process::Command::new(env!("CARGO_BIN_EXE_git-tools"));
        command
            .args(arguments)
            .current_dir(&self.repository)
            .env("NO_COLOR", "1")
            .env("GIT_TOOLS_NO_OPEN", "1");
        command
    }

    fn run(&self, arguments: &[&str]) -> Command {
        let mut command = Command::from_std(self.command(arguments));
        command.timeout(Duration::from_secs(15));
        command
    }

    fn git_command(&self) -> process::Command {
        let mut command = process::Command::new("git");
        command.arg("-C").arg(&self.repository);
        command
    }

    fn git(&self, arguments: &[&str]) -> Result<String> {
        run_git(self.git_command().args(arguments))
    }
}

fn run_git(command: &mut process::Command) -> Result<String> {
    let result = command
        .output()
        .with_context(|| format!("run {command:?}"))?;
    ensure!(
        result.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(String::from_utf8(result.stdout)?.trim().to_string())
}

#[track_caller]
pub fn assert_contains(output: &str, fragments: &[&str]) {
    for fragment in fragments {
        assert!(
            output.contains(fragment),
            "missing {fragment:?} in:\n{output}"
        );
    }
}

#[track_caller]
pub fn assert_omits(output: &str, fragments: &[&str]) {
    for fragment in fragments {
        assert!(
            !output.contains(fragment),
            "unexpected {fragment:?} in:\n{output}"
        );
    }
}
