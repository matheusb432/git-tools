use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use gtl_application::history::record_render::{self, RecordRender};
use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};
use gtl_infra::{app_state::SqliteAppState, clock::SystemClock};
use serde::Deserialize;

const CONCURRENT_LIVE_LINE_COUNT_MAX: usize = 45_000;
const RENDER_HISTORY_SEED_COUNT_MAX: usize = 60;
const ONE_SHOT_ALPHA_REPOSITORY: &str = "one-shot-alpha";
const ONE_SHOT_BETA_REPOSITORY: &str = "one-shot-beta";

const ENVIRONMENT_VARIABLES: [&str; 6] = [
    "GTL_E2E_CLI_BINARY",
    "GTL_E2E_FIXTURE_ROOT",
    "GTL_E2E_EDITOR_RECORDER",
    "GTL_E2E_EDITOR_RECORD",
    "GTL_E2E_EDITOR_RELEASE",
    "GTL_E2E_EDITOR_EXIT",
];

#[derive(Debug, Deserialize)]
pub struct EditorRecord {
    pub pid: u32,
    pub working_directory: String,
    pub arguments: Vec<String>,
}

pub struct ViewerFixture {
    repository: PathBuf,
    concurrent_live_root: PathBuf,
    unavailable_repository: PathBuf,
    cli: PathBuf,
    data_root: PathBuf,
    editor_record: PathBuf,
    editor_release: PathBuf,
    editor_exit: PathBuf,
}

pub struct OneShotFixture {
    alpha_repository: PathBuf,
    beta_repository: PathBuf,
    cli: PathBuf,
    data_root: PathBuf,
}

impl OneShotFixture {
    pub fn create(data_root: &Path) -> Result<Self> {
        let cli = required_environment_path("GTL_E2E_CLI_BINARY")?;
        let fixture_root = required_environment_path("GTL_E2E_FIXTURE_ROOT")?;
        let repositories = fixture_root.join("dom-repositories");
        let alpha_repository = create_snapshot_repository(
            &repositories,
            ONE_SHOT_ALPHA_REPOSITORY,
            "alpha-one-shot-marker",
        )?;
        let beta_repository = create_snapshot_repository(
            &repositories,
            ONE_SHOT_BETA_REPOSITORY,
            "beta-one-shot-marker",
        )?;
        Ok(Self {
            alpha_repository,
            beta_repository,
            cli,
            data_root: data_root.to_path_buf(),
        })
    }

    pub fn forward_alpha(&self) -> Result<()> {
        self.forward(&self.alpha_repository)
            .context("forward alpha one-shot diff")
    }

    pub fn forward_beta(&self) -> Result<()> {
        self.forward(&self.beta_repository)
            .context("forward beta one-shot diff")
    }

    pub fn seed_render_history(&self, count: usize) -> Result<()> {
        if count > RENDER_HISTORY_SEED_COUNT_MAX {
            bail!(
                "render-history fixture requested {count} rows; maximum is {RENDER_HISTORY_SEED_COUNT_MAX}"
            );
        }

        let app_state = SqliteAppState::open(&self.data_root).context("open fixture app state")?;
        let mut connection = app_state
            .connection_lock()
            .context("lock fixture app state")?;
        for index in 1..=count {
            let repo_name = format!("history-seed-{index:02}");
            let recipe = Recipe {
                source: RecipeSource::LocalRepo(
                    PathBuf::from("/e2e/render-history").join(&repo_name),
                ),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                name: None,
            };
            record_render::execute(
                RecordRender {
                    recipe,
                    title: format!("{repo_name}: 1 commit"),
                    repo_name,
                    range_label: "main".into(),
                },
                &mut connection,
                &SystemClock,
            )
            .with_context(|| format!("seed render-history row {index}"))?;
        }
        Ok(())
    }

    fn forward(&self, repository: &Path) -> Result<()> {
        command_checked_with_data_root(&self.cli, ["diff"], Some(repository), &self.data_root)
    }
}

impl ViewerFixture {
    pub fn create(data_root: &Path) -> Result<Self> {
        let [
            cli,
            fixture_root,
            editor_recorder,
            editor_record,
            editor_release,
            editor_exit,
        ] = required_environment_paths()?;
        let repository = fixture_root.join("dom-repositories/live-view");
        let concurrent_live_root = fixture_root.join("dom-repositories/concurrent-live");
        let unavailable_repository = fixture_root.join("dom-repositories/live-view-unavailable");
        fs::create_dir_all(&repository)
            .with_context(|| format!("create live-view repository {}", repository.display()))?;

        git(&repository, ["init", "-q", "-b", "main"])?;
        git(&repository, ["config", "user.name", "Viewer E2E"])?;
        git(
            &repository,
            ["config", "user.email", "viewer-e2e@example.invalid"],
        )?;
        let editor = configured_editor(
            &editor_recorder,
            &editor_record,
            &editor_release,
            &editor_exit,
        );
        git(&repository, ["config", "core.editor", editor.as_str()])?;
        fs::write(repository.join("work.txt"), "base\n").context("write base worktree")?;
        git(&repository, ["add", "work.txt"])?;
        git(&repository, ["commit", "-q", "-m", "base"])?;
        git(&repository, ["switch", "-q", "-c", "feature"])?;
        fs::write(repository.join("work.txt"), "base\nalpha-v1\n")
            .context("write alpha-v1 worktree")?;
        git(&repository, ["add", "work.txt"])?;
        git(&repository, ["commit", "-q", "-m", "live view v1"])?;

        Ok(Self {
            repository,
            concurrent_live_root,
            unavailable_repository,
            cli,
            data_root: data_root.to_path_buf(),
            editor_record,
            editor_release,
            editor_exit,
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

    pub fn forward_sized_live_view(
        &self,
        repository_name: &str,
        marker: &str,
        added_line_count: usize,
    ) -> Result<()> {
        if added_line_count > CONCURRENT_LIVE_LINE_COUNT_MAX {
            bail!(
                "live-view fixture requested {added_line_count} lines; maximum is {CONCURRENT_LIVE_LINE_COUNT_MAX}"
            );
        }
        let repository = self.concurrent_live_root.join(repository_name);
        fs::create_dir_all(&repository)
            .with_context(|| format!("create live-view repository {}", repository.display()))?;
        git(&repository, ["init", "-q", "-b", "main"])?;
        git(&repository, ["config", "user.name", "Viewer E2E"])?;
        git(
            &repository,
            ["config", "user.email", "viewer-e2e@example.invalid"],
        )?;
        fs::write(repository.join("work.txt"), "base\n").context("write live-view base")?;
        git(&repository, ["add", "work.txt"])?;
        git(&repository, ["commit", "-q", "-m", "live-view base"])?;
        git(&repository, ["switch", "-q", "-c", "feature"])?;
        let contents = format!("{marker}\n{}", "changed\n".repeat(added_line_count));
        fs::write(repository.join("work.txt"), contents)
            .with_context(|| format!("write {repository_name} live view"))?;
        git(&repository, ["add", "work.txt"])?;
        git(&repository, ["commit", "-q", "-m", "live-view change"])?;
        command_checked_with_data_root(
            &self.cli,
            ["diff", "live", "--path", path_as_str(&repository)?],
            None,
            &self.data_root,
        )
        .with_context(|| format!("forward {repository_name} live view through release CLI"))
    }

    pub fn make_repository_unavailable(&self) -> Result<()> {
        fs::rename(&self.repository, &self.unavailable_repository).with_context(|| {
            format!(
                "move live-view repository from {} to {}",
                self.repository.display(),
                self.unavailable_repository.display()
            )
        })
    }

    pub fn restore_repository(&self) -> Result<()> {
        fs::rename(&self.unavailable_repository, &self.repository).with_context(|| {
            format!(
                "restore live-view repository from {} to {}",
                self.unavailable_repository.display(),
                self.repository.display()
            )
        })
    }

    pub fn canonical_repository(&self) -> Result<PathBuf> {
        self.repository
            .canonicalize()
            .with_context(|| format!("canonicalize repository {}", self.repository.display()))
    }

    pub fn editor_record(&self) -> Result<EditorRecord> {
        let bytes = fs::read(&self.editor_record)
            .with_context(|| format!("read editor record {}", self.editor_record.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("parse editor record {}", self.editor_record.display()))
    }

    pub fn editor_record_exists(&self) -> bool {
        self.editor_record.is_file()
    }

    pub fn editor_exit_exists(&self) -> bool {
        self.editor_exit.is_file()
    }

    pub fn release_editor(&self, process_id: u32) -> Result<()> {
        if process_id == 0 {
            bail!("editor recorder reported an invalid process id");
        }
        fs::write(&self.editor_release, b"release")
            .with_context(|| format!("release editor recorder {process_id}"))
    }
}

fn required_environment_paths() -> Result<[PathBuf; 6]> {
    let paths = ENVIRONMENT_VARIABLES.map(required_environment_path);
    let [
        cli,
        fixture_root,
        editor_recorder,
        editor_record,
        editor_release,
        editor_exit,
    ] = paths;
    Ok([
        cli?,
        fixture_root?,
        editor_recorder?,
        editor_record?,
        editor_release?,
        editor_exit?,
    ])
}

fn required_environment_path(name: &str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{name} is required"))
}

fn create_snapshot_repository(root: &Path, name: &str, marker: &str) -> Result<PathBuf> {
    let repository = root.join(name);
    fs::create_dir_all(&repository)
        .with_context(|| format!("create one-shot repository {}", repository.display()))?;
    git(&repository, ["init", "-q", "-b", "main"])?;
    git(&repository, ["config", "user.name", "Viewer E2E"])?;
    git(
        &repository,
        ["config", "user.email", "viewer-e2e@example.invalid"],
    )?;
    fs::write(repository.join("work.txt"), "base\n").context("write one-shot base")?;
    git(&repository, ["add", "work.txt"])?;
    git(&repository, ["commit", "-q", "-m", "base"])?;
    git(&repository, ["switch", "-q", "-c", "feature"])?;
    fs::write(repository.join("work.txt"), format!("base\n{marker}\n"))
        .with_context(|| format!("write {name} one-shot change"))?;
    git(&repository, ["add", "work.txt"])?;
    git(&repository, ["commit", "-q", "-m", "one-shot change"])?;
    Ok(repository)
}

fn configured_editor(recorder: &Path, record: &Path, release: &Path, exit: &Path) -> String {
    format!(
        "\"{}\" \"{}\" \"{}\" \"{}\" --wait --profile \"Viewer E2E\"",
        recorder.display(),
        record.display(),
        release.display(),
        exit.display()
    )
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
