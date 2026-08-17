//! Selective checks over content staged in the Git index.

use std::{env, path::Path};

use anyhow::{Context, Result};

use crate::{process, task::Step};

mod checks;
mod snapshot;

use checks::CheckPaths;

pub(crate) fn run() -> Result<()> {
    process::run_step(&Step::new(
        "staged-whitespace",
        "git",
        ["diff", "--cached", "--check"],
    ))?;
    let Some(snapshot) = snapshot::StagedSnapshot::create()? else {
        return Ok(());
    };

    let checks = CheckPaths::classify(snapshot.paths());
    let mut steps = Vec::new();
    if !checks.rust.is_empty() {
        let rustfmt = env::var("RUSTFMT").context("RUSTFMT is not configured")?;
        steps.push(
            crate::task::Step::new(
                "rustfmt",
                rustfmt,
                [
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
    if !checks.dioxus.is_empty() {
        steps.push(crate::verbs::format::dioxus_check_step(
            snapshot.directory(),
        ));
    }
    if !checks.toml.is_empty() {
        steps.push(
            crate::task::Step::new("taplo", "taplo", ["fmt", "--check"])
                .with_arguments(checks.toml.iter().map(|path| path_text(path)))
                .with_current_directory(snapshot.directory()),
        );
    }
    if !checks.markdown.is_empty() {
        steps.push(
            Step::new("rumdl", "rumdl", ["fmt", "--check"])
                .with_arguments(checks.markdown.iter().map(|path| path_text(path)))
                .with_current_directory(snapshot.directory()),
        );
    }
    for step in &steps {
        process::run_step(step)?;
    }
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
