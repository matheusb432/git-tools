//! Selective checks over content staged in the Git index.

use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::{
    process::{self, Status},
    task,
    verb::Verb,
};

mod checks;
mod snapshot;

use checks::CheckPaths;

pub(crate) fn run() -> Result<()> {
    process::run("staged-whitespace", "git", &["diff", "--cached", "--check"])?;
    let Some(snapshot) = snapshot::StagedSnapshot::create()? else {
        process::result(Verb::PRE_COMMIT, Status::Pass);
        return Ok(());
    };

    let checks = CheckPaths::classify(snapshot.paths());
    let mut steps = Vec::new();
    if !checks.rust.is_empty() {
        let toolchain_nightly = fs::read_to_string(snapshot.directory().join(".rustfmt-nightly"))
            .context("reading staged .rustfmt-nightly")?;
        steps.push(
            crate::task::Step::new(
                "rustfmt",
                "rustup",
                [
                    "run".to_string(),
                    toolchain_nightly.trim().to_string(),
                    "rustfmt".to_string(),
                    "--check".to_string(),
                    "--unstable-features".to_string(),
                    "--skip-children".to_string(),
                    "--config-path".to_string(),
                    "rustfmt.toml".to_string(),
                ],
            )
            .with_arguments(checks.rust.iter().map(|path| path_text(path)))
            .with_current_directory(snapshot.directory()),
        );
    }
    if !checks.toml.is_empty() {
        steps.push(
            crate::task::Step::new("taplo", "taplo", ["fmt", "--check"])
                .with_arguments(checks.toml.iter().map(|path| path_text(path)))
                .with_current_directory(snapshot.directory()),
        );
    }
    if !checks.markdown.is_empty() {
        steps.push(crate::verbs::format::markdown::check_step_for_files(
            checks.markdown,
            snapshot.directory(),
        ));
    }
    if !checks.frontend_format.is_empty() {
        steps.push(
            crate::task::Step::new(
                "frontend-format",
                "deno",
                [
                    "task".to_string(),
                    "--frozen".to_string(),
                    "format:check:files".to_string(),
                    "--".to_string(),
                    format!(
                        "--config={}",
                        snapshot.directory().join(".oxfmtrc.json").display()
                    ),
                ],
            )
            .with_arguments(
                checks
                    .frontend_format
                    .into_iter()
                    .map(|path| snapshot.directory().join(path))
                    .map(|path| path_text(&path)),
            ),
        );
    }
    if !checks.frontend_lint.is_empty() {
        steps.push(
            crate::task::Step::new(
                "frontend-lint",
                "deno",
                [
                    "task".to_string(),
                    "--frozen".to_string(),
                    "lint:files".to_string(),
                    "--".to_string(),
                    format!(
                        "--config={}",
                        snapshot.directory().join(".oxlintrc.json").display()
                    ),
                ],
            )
            .with_arguments(
                checks
                    .frontend_lint
                    .into_iter()
                    .map(|path| snapshot.directory().join(path))
                    .map(|path| path_text(&path)),
            ),
        );
    }
    task::run_all(&steps)?;

    process::result(Verb::PRE_COMMIT, Status::Pass);
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
