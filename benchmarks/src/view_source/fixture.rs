use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};

use super::ViewSourceWorkload;
use crate::desktop_scroll;

pub(super) const SPARSE_SOURCE_LINE_COUNT: usize = 20_000;
pub(super) const SPARSE_CHANGE_STRIDE: usize = 200;

const GIT_DIAGNOSTIC_BYTES_MAX: usize = 64 * 1_024;
const IDENTITY_NAME: &str = "View Source Benchmark";
const IDENTITY_EMAIL: &str = "view-source@example.invalid";
const BASE_TIMESTAMP: &str = "2026-03-01T12:00:00+00:00";
const CHANGE_TIMESTAMP: &str = "2026-03-02T12:00:00+00:00";
const GIT_ENVIRONMENT_VARIABLES_REMOVED: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG_COUNT",
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_WORK_TREE",
];

pub(super) struct MaterializedFixture {
    pub workload: ViewSourceWorkload,
    pub name: String,
    pub repository: PathBuf,
    pub range: String,
}

pub(super) fn materialize(
    workload: ViewSourceWorkload,
    destination: &Path,
) -> Result<MaterializedFixture> {
    match workload {
        ViewSourceWorkload::ManyModifiedFiles => materialize_many_files(destination),
        ViewSourceWorkload::SparseLargeModifiedFile => materialize_sparse_file(destination),
    }
}

fn materialize_many_files(destination: &Path) -> Result<MaterializedFixture> {
    let manifest = desktop_scroll::hydrate_fixture(&desktop_scroll::fixture_root(), destination)
        .context("hydrate committed many-file fixture")?;
    Ok(MaterializedFixture {
        workload: ViewSourceWorkload::ManyModifiedFiles,
        name: manifest.fixture_name,
        repository: destination.to_path_buf(),
        range: manifest.git_range,
    })
}

fn materialize_sparse_file(destination: &Path) -> Result<MaterializedFixture> {
    ensure!(
        !destination.exists(),
        "sparse fixture destination already exists: {}",
        destination.display()
    );
    fs::create_dir_all(destination)
        .with_context(|| format!("create sparse fixture at {}", destination.display()))?;
    git_run(
        destination,
        "initialize sparse repository",
        ["init", "-q", "-b", "main"],
        None,
    )?;
    for (key, value) in [
        ("user.name", IDENTITY_NAME),
        ("user.email", IDENTITY_EMAIL),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
        ("core.fileMode", "false"),
        ("diff.renames", "false"),
    ] {
        git_run(
            destination,
            "configure sparse repository",
            ["config", key, value],
            None,
        )?;
    }

    let source = destination.join("src/large.rs");
    fs::create_dir_all(
        source
            .parent()
            .context("sparse fixture source has no parent")?,
    )
    .context("create sparse fixture source directory")?;
    fs::write(&source, sparse_source(false)).context("write sparse fixture base source")?;
    commit_all(
        destination,
        "fixture: establish sparse large source",
        BASE_TIMESTAMP,
    )?;
    fs::write(&source, sparse_source(true)).context("write sparse fixture changed source")?;
    commit_all(
        destination,
        "fixture: sparsely modify large source",
        CHANGE_TIMESTAMP,
    )?;

    Ok(MaterializedFixture {
        workload: ViewSourceWorkload::SparseLargeModifiedFile,
        name: "sparse-large-modified-file-v1".to_owned(),
        repository: destination.to_path_buf(),
        range: "HEAD~1..HEAD".to_owned(),
    })
}

fn sparse_source(changed: bool) -> String {
    let mut source = String::with_capacity(SPARSE_SOURCE_LINE_COUNT.saturating_mul(48));
    for line in 1..=SPARSE_SOURCE_LINE_COUNT {
        let value = if changed && line % SPARSE_CHANGE_STRIDE == 0 {
            line.saturating_add(1_000_000)
        } else {
            line
        };
        let _ = writeln!(
            source,
            "pub const ROW_{line:05}: usize = {value}; // stable benchmark source"
        );
    }
    source
}

fn commit_all(root: &Path, subject: &str, timestamp: &str) -> Result<()> {
    git_run(root, "stage sparse fixture", ["add", "-A"], None)?;
    git_run(
        root,
        "commit sparse fixture",
        ["commit", "-q", "--no-gpg-sign", "-m", subject],
        Some(timestamp),
    )
}

fn git_run<const N: usize>(
    root: &Path,
    operation: &str,
    arguments: [&str; N],
    timestamp: Option<&str>,
) -> Result<()> {
    let mut command = Command::new("git");
    command.args(arguments).current_dir(root);
    for variable in GIT_ENVIRONMENT_VARIABLES_REMOVED {
        command.env_remove(variable);
    }
    if let Some(timestamp) = timestamp {
        command
            .env("GIT_AUTHOR_DATE", timestamp)
            .env("GIT_COMMITTER_DATE", timestamp);
    }
    let output = command
        .output()
        .with_context(|| format!("{operation} in {}", root.display()))?;
    ensure!(
        output.stdout.len() <= GIT_DIAGNOSTIC_BYTES_MAX
            && output.stderr.len() <= GIT_DIAGNOSTIC_BYTES_MAX,
        "{operation} output exceeded {GIT_DIAGNOSTIC_BYTES_MAX} bytes"
    );
    if !output.status.success() {
        bail!(
            "{operation} failed with exit {}: {}{}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
