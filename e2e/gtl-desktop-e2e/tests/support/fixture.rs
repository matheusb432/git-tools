use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};

const ONE_SHOT_REPOSITORY: &str = "one-shot-alpha";
const TAB_OVERFLOW_INITIAL_COUNT: usize = 4;
const TAB_OVERFLOW_TOTAL_COUNT: usize = 12;

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

pub struct TabOverflowFixture {
    entries: Vec<TabOverflowEntry>,
    cli: PathBuf,
    data_root: PathBuf,
}

struct TabOverflowEntry {
    repository: PathBuf,
    name: String,
    marker: String,
}

impl OneShotFixture {
    pub fn create(data_root: &Path) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let fixture_root = required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
        let repository = create_changed_repository(
            &fixture_root.join("dom-repositories"),
            ONE_SHOT_REPOSITORY,
            &format!(
                "alpha-one-shot-marker\n{}",
                "scrollbar fixture\n".repeat(120)
            ),
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
}

impl TabOverflowFixture {
    pub fn create(data_root: &Path) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let fixture_root = required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
        let repositories_root = fixture_root.join("dom-repositories");
        let entries = (1..=TAB_OVERFLOW_TOTAL_COUNT)
            .map(|index| {
                let name = format!("sample-set-{index:02}");
                let marker = format!("sample-marker-{index:02}");
                let change_message = format!("sample change {index:02}");
                let repository =
                    create_changed_repository(&repositories_root, &name, &marker, &change_message)?;
                Ok(TabOverflowEntry {
                    repository,
                    name,
                    marker,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            entries,
            cli,
            data_root: data_root.to_path_buf(),
        })
    }

    pub fn forward_initial(&self) -> Result<()> {
        let (initial, _) = self.entries.split_at(TAB_OVERFLOW_INITIAL_COUNT);
        self.forward_entries(initial)
    }

    pub fn forward_remaining(&self) -> Result<()> {
        let (_, remaining) = self.entries.split_at(TAB_OVERFLOW_INITIAL_COUNT);
        self.forward_entries(remaining)
    }

    fn forward_entries(&self, entries: &[TabOverflowEntry]) -> Result<()> {
        for entry in entries {
            command_checked_with_data_root(
                &self.cli,
                ["diff"],
                Some(&entry.repository),
                &self.data_root,
            )
            .with_context(|| format!("forward snapshot for {}", entry.name))?;
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn first_identity(&self) -> Option<(&str, &str)> {
        self.entries
            .first()
            .map(|entry| (entry.name.as_str(), entry.marker.as_str()))
    }

    pub fn initial_last_identity(&self) -> Option<(&str, &str)> {
        self.entries
            .get(TAB_OVERFLOW_INITIAL_COUNT - 1)
            .map(|entry| (entry.name.as_str(), entry.marker.as_str()))
    }

    pub fn last_identity(&self) -> Option<(&str, &str)> {
        self.entries
            .last()
            .map(|entry| (entry.name.as_str(), entry.marker.as_str()))
    }
}

impl ViewerFixture {
    pub fn create(data_root: &Path) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let fixture_root = required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
        let repository = create_changed_repository(
            &fixture_root.join("dom-repositories"),
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
