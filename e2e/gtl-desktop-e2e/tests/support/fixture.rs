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
        Self::create_named(data_root, ONE_SHOT_REPOSITORY)
    }

    pub fn create_named(data_root: &Path, name: &str) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let fixture_root = required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
        let repository = create_changed_repository(
            &fixture_root.join("dom-repositories"),
            name,
            &format!(
                "alpha-one-shot-marker\nwrapping-source {}\n{}",
                "readable source text ".repeat(15),
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

    pub fn create_with_excluded_files(data_root: &Path, name: &str) -> Result<Self> {
        let mut fixture = Self::create_named(data_root, name)?;
        let root = required_environment_path("HOME")?.join("extension-fixtures");
        fs::create_dir_all(&root)?;
        let repository = root.join(name);
        fs::rename(&fixture.repository, &repository)?;
        fixture.repository = repository;
        fs::write(
            fixture.repository.join("Cargo.lock"),
            "excluded-lock-marker\n",
        )?;
        git(&fixture.repository, ["add", "Cargo.lock"])?;
        git(
            &fixture.repository,
            ["commit", "--amend", "--no-edit", "-q"],
        )?;
        Ok(fixture)
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }

    pub fn create_with_panel_history(data_root: &Path, name: &str) -> Result<Self> {
        let fixture = Self::create_named(data_root, name)?;
        for index in 0..48 {
            let filename = format!("panel-{index:02}.txt");
            fs::write(fixture.repository.join(&filename), "panel scroll fixture\n")?;
            git(&fixture.repository, ["add", &filename])?;
            git(
                &fixture.repository,
                ["commit", "-q", "-m", &format!("Add panel file {index:02}")],
            )?;
        }
        Ok(fixture)
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
        Self::create_named(data_root, "live-view")
    }

    pub fn create_named(data_root: &Path, name: &str) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let fixture_root = required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
        let repository = create_changed_repository(
            &fixture_root.join("dom-repositories"),
            name,
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

    pub fn make_git_unavailable(&self) -> Result<()> {
        fs::rename(
            self.repository.join(".git"),
            self.repository.join("git-unavailable"),
        )?;
        Ok(())
    }

    pub fn restore_git(&self) -> Result<()> {
        fs::rename(
            self.repository.join("git-unavailable"),
            self.repository.join(".git"),
        )?;
        Ok(())
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
    pub alpha: PathBuf,
    pub beta: PathBuf,
    pub initial: PathBuf,
    pub missing: PathBuf,
}

pub struct PushFixture {
    pub repository: PathBuf,
    remote: PathBuf,
    pub first: String,
    pub latest: String,
}

pub struct PushGate {
    gate: PathBuf,
    entered: PathBuf,
}

impl PushGate {
    pub fn entered(&self) -> &Path {
        &self.entered
    }

    pub fn release(&self) -> Result<()> {
        fs::remove_file(&self.gate).context("release fixture push gate")
    }
}

impl Drop for PushGate {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.gate);
    }
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
        let first = Self::revision(&repository, "HEAD")?;
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
            first,
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

    pub fn remote_head(&self) -> Result<String> {
        Self::revision(&self.remote, "main")
    }

    pub fn hold_remote_push(&self) -> Result<PushGate> {
        let hooks = self.remote.join("hooks");
        let gate = hooks.join("push-gate");
        let entered = hooks.join("push-entered");
        fs::write(&gate, "")?;
        let hook = hooks.join("pre-receive");
        fs::write(
            &hook,
            "#!/bin/sh\nhook_dir=\"$(dirname \"$0\")\"\ntouch \"$hook_dir/push-entered\"\nattempt=0\nwhile [ -e \"$hook_dir/push-gate\" ] && [ \"$attempt\" -lt 300 ]; do\n  sleep 0.1\n  attempt=$((attempt + 1))\ndone\n[ ! -e \"$hook_dir/push-gate\" ]\n",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        }
        Ok(PushGate { gate, entered })
    }

    pub fn add_newer(&self) -> Result<()> {
        fs::write(
            self.repository.join("work.txt"),
            "base\npush-first-marker\npush-latest-marker\npush-newer-marker\n",
        )?;
        git(&self.repository, ["add", "work.txt"])?;
        git(&self.repository, ["commit", "-qm", "push newer"])
    }

    pub fn append_commit(&self, name: &str) -> Result<String> {
        let path = self.repository.join("work.txt");
        let mut content = fs::read_to_string(&path)?;
        content.push_str(name);
        content.push('\n');
        fs::write(path, content)?;
        git(&self.repository, ["add", "work.txt"])?;
        git(&self.repository, ["commit", "-qm", name])?;
        Self::revision(&self.repository, "HEAD")
    }

    pub fn rewrite(&self) -> Result<()> {
        git(&self.repository, ["reset", "--soft", "origin/main"])?;
        git(&self.repository, ["commit", "-qm", "push rewritten"])
    }

    pub fn soft_reset(&self, commit: &str) -> Result<()> {
        git(&self.repository, ["reset", "--soft", commit])
    }
}

impl ProjectsFixture {
    pub fn use_local_comparison(&self) -> Result<()> {
        git(&self.alpha, ["branch", "review-base", "main"])?;
        git(&self.alpha, ["branch", "--unset-upstream"])
    }

    pub fn restore_upstream(&self) -> Result<()> {
        git(&self.alpha, ["branch", "--set-upstream-to=main"])
    }

    pub fn create(data_root: &Path) -> Result<Self> {
        let root = data_root.join("project-repositories");
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
        let initial = root.join("projects-initial");
        fs::create_dir_all(&initial)?;
        git(&initial, ["init", "-q", "-b", "main"])?;
        fs::write(initial.join("first.txt"), "initial-project-marker\n")?;
        Ok(Self {
            alpha,
            beta,
            initial,
            missing: root.join("missing"),
        })
    }

    pub fn change_untracked(&self) -> Result<()> {
        fs::write(self.alpha.join("new.txt"), "refreshed-project-marker\n")
            .context("update untracked fixture")?;
        git(
            &self.alpha,
            [
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "update local comparison HEAD",
            ],
        )
    }
}
