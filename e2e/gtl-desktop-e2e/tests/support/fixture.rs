use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};

pub struct ViewerFixture {
    repository: PathBuf,
    cli: PathBuf,
    data_root: PathBuf,
}

pub struct OneShotFixture {
    repository: PathBuf,
    cli: PathBuf,
    data_root: PathBuf,
}

impl OneShotFixture {
    pub fn create_named(data_root: &Path, name: &str) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let repository = create_changed_repository(
            &data_root.join("repositories"),
            name,
            "alpha-one-shot-marker",
            "one-shot change",
        )?;
        Ok(Self {
            repository,
            cli,
            data_root: data_root.to_path_buf(),
        })
    }

    pub fn forward(&self) -> Result<()> {
        command_checked_with_data_root(&self.cli, ["diff"], Some(&self.repository), &self.data_root)
            .context("forward one-shot diff")
    }

    pub fn create_with_long_lines(data_root: &Path) -> Result<Self> {
        let fixture = Self::create_named(data_root, "long-lines")?;
        for (path, characters) in [("a.css", 94_718), ("b.css", 90_015)] {
            fs::write(
                fixture.repository.join(path),
                format!("{}\n", "x".repeat(characters)),
            )?;
        }
        git(&fixture.repository, ["add", "."])?;
        git(
            &fixture.repository,
            ["commit", "-q", "-m", "Add generated CSS"],
        )?;
        Ok(fixture)
    }
}

impl ViewerFixture {
    pub fn create(data_root: &Path) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let repository = create_changed_repository(
            &data_root.join("repositories"),
            "live-view",
            "alpha-v1",
            "live view v1",
        )?;
        Ok(Self {
            repository,
            cli,
            data_root: data_root.to_path_buf(),
        })
    }

    pub fn forward_live_view(&self) -> Result<()> {
        command_checked_with_data_root(
            &self.cli,
            ["diff", "live", "--path", path_as_str(&self.repository)?],
            None,
            &self.data_root,
        )
        .context("forward live view through release CLI")
    }

    pub fn commit_extra(&self) -> Result<()> {
        fs::write(
            self.repository.join("extra.txt"),
            "additional-live-marker\n",
        )?;
        git(&self.repository, ["add", "extra.txt"])?;
        git(&self.repository, ["commit", "-q", "-m", "live view extra"])
    }

    pub fn commit_alpha_v2(&self) -> Result<()> {
        fs::write(self.repository.join("work.txt"), "base\nalpha-v2\n")
            .context("write alpha-v2 worktree")?;
        git(&self.repository, ["add", "work.txt"])?;
        git(&self.repository, ["commit", "-q", "-m", "live view v2"])
    }
}

fn required_environment_path(name: &str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{name} is required"))
}

fn create_changed_repository(
    root: &Path,
    name: &str,
    marker: &str,
    change_message: &str,
) -> Result<PathBuf> {
    let repository = root.join(name);
    fs::create_dir_all(&repository)
        .with_context(|| format!("create fixture repository {}", repository.display()))?;
    git(&repository, ["init", "-q", "-b", "main"])?;
    git(&repository, ["config", "user.name", "Viewer E2E"])?;
    git(
        &repository,
        ["config", "user.email", "viewer-e2e@example.invalid"],
    )?;
    fs::write(repository.join("work.txt"), "base\n").context("write fixture base")?;
    git(&repository, ["add", "work.txt"])?;
    git(&repository, ["commit", "-q", "-m", "base"])?;
    git(&repository, ["switch", "-q", "-c", "feature"])?;
    fs::write(repository.join("work.txt"), format!("base\n{marker}\n"))
        .with_context(|| format!("write {name} fixture change"))?;
    git(&repository, ["add", "work.txt"])?;
    git(&repository, ["commit", "-q", "-m", change_message])?;
    Ok(repository)
}

fn git<const N: usize>(repository: &Path, arguments: [&str; N]) -> Result<()> {
    command_checked("git", arguments, Some(repository))
}

fn command_checked<const N: usize>(
    program: impl AsRef<Path>,
    arguments: [&str; N],
    working_directory: Option<&Path>,
) -> Result<()> {
    command_checked_with_optional_data_root(program, arguments, working_directory, None)
}

fn command_checked_with_data_root<const N: usize>(
    program: impl AsRef<Path>,
    arguments: [&str; N],
    working_directory: Option<&Path>,
    data_root: &Path,
) -> Result<()> {
    command_checked_with_optional_data_root(program, arguments, working_directory, Some(data_root))
}

fn command_checked_with_optional_data_root<const N: usize>(
    program: impl AsRef<Path>,
    arguments: [&str; N],
    working_directory: Option<&Path>,
    data_root: Option<&Path>,
) -> Result<()> {
    let mut command = Command::new(program.as_ref());
    command.args(arguments);
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
    }
    if let Some(data_root) = data_root {
        command.env("GIT_TOOLS_DATA_DIR", data_root);
    }
    let output = command
        .output()
        .with_context(|| format!("run {}", program.as_ref().display()))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "{} failed (exit {}): {}{}",
        program.as_ref().display(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn path_as_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}

pub struct ProjectsFixture {
    pub root: PathBuf,
}

pub struct PushFixture {
    pub repository: PathBuf,
    remote: PathBuf,
    pub latest: String,
}

impl PushFixture {
    pub fn create(root: &Path) -> Result<Self> {
        let repository =
            create_changed_repository(root, "push-review", "push-first-marker", "push first")?;
        let remote = root.join("push-remote.git");
        git(
            root,
            [
                "clone",
                "--bare",
                "--quiet",
                path_as_str(&repository)?,
                path_as_str(&remote)?,
            ],
        )?;
        git(
            &repository,
            ["remote", "add", "origin", path_as_str(&remote)?],
        )?;
        git(&repository, ["fetch", "-q", "origin"])?;
        git(&repository, ["branch", "--set-upstream-to=origin/main"])?;
        fs::write(
            repository.join("work.txt"),
            "base\npush-first-marker\npush-latest-marker\n",
        )?;
        git(&repository, ["add", "work.txt"])?;
        git(&repository, ["commit", "-qm", "push latest"])?;
        let latest = Self::revision(&repository, "HEAD")?;
        fs::write(repository.join("untracked.txt"), "keep local\n")?;
        Ok(Self {
            repository,
            remote,
            latest,
        })
    }

    fn revision(path: &Path, revision: &str) -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["rev-parse", revision])
            .output()?;
        anyhow::ensure!(output.status.success(), "read fixture revision");
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }

    pub fn forward_snapshot(&self, data_root: &Path) -> Result<()> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        command_checked_with_data_root(cli, ["diff"], Some(&self.repository), data_root)
            .context("forward push snapshot")
    }

    pub fn remote_head(&self) -> Result<String> {
        Self::revision(&self.remote, "main")
    }

    pub fn add_newer(&self) -> Result<()> {
        fs::write(
            self.repository.join("work.txt"),
            "base\npush-first-marker\npush-latest-marker\npush-newer-marker\n",
        )?;
        git(&self.repository, ["add", "work.txt"])?;
        git(&self.repository, ["commit", "-qm", "push newer"])
    }
}

impl ProjectsFixture {
    /// Creates a scan root holding a repository with unpushed and uncommitted work and a
    /// clean one.
    pub fn create(parent: &Path) -> Result<Self> {
        let root = parent.join("project-repositories");
        fs::create_dir_all(&root)?;
        let alpha = create_changed_repository(
            &root,
            "projects-alpha",
            "committed-project-marker",
            "committed project work",
        )?;
        git(&alpha, ["branch", "--set-upstream-to=main"])?;
        fs::write(
            alpha.join("work.txt"),
            "base\ncommitted-project-marker\nlocal-project-marker\n",
        )?;
        fs::write(alpha.join("new.txt"), "untracked-project-marker\n")?;
        let beta = create_changed_repository(
            &root,
            "projects-beta",
            "clean-project-marker",
            "clean project work",
        )?;
        git(&beta, ["branch", "-f", "main", "HEAD"])?;
        git(&beta, ["branch", "--set-upstream-to=main"])?;
        Ok(Self { root })
    }
}
