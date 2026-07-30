use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

const CONCURRENT_LIVE_LINE_COUNT_MAX: usize = 45_000;

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
    cli: PathBuf,
    editor_record: PathBuf,
    editor_release: PathBuf,
    editor_exit: PathBuf,
}

impl ViewerFixture {
    pub fn create() -> Result<Self> {
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
            cli,
            editor_record,
            editor_release,
            editor_exit,
        })
    }

    pub fn forward_live_view(&self) -> Result<()> {
        command_checked(
            &self.cli,
            ["diff", "live", "--path", path_as_str(&self.repository)?],
            None,
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
        command_checked(
            &self.cli,
            ["diff", "live", "--path", path_as_str(&repository)?],
            None,
        )
        .with_context(|| format!("forward {repository_name} live view through release CLI"))
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
    let paths = ENVIRONMENT_VARIABLES.map(|name| {
        env::var_os(name)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("{name} is required"))
    });
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
    let mut command = Command::new(program.as_ref());
    command.args(arguments);
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
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
