use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};

const GIT_EDITOR_QUERY_TIMEOUT: Duration = Duration::from_secs(3);
const GIT_EDITOR_COMMAND_BYTES_MAX: usize = 16 * 1024;

pub(super) fn read_configured_command(repository_root: &Path) -> Result<String> {
    let output = super::process::run_command_with_bounded_stdout_in(
        Path::new("git"),
        &["var", "GIT_EDITOR"],
        repository_root,
        GIT_EDITOR_QUERY_TIMEOUT,
        GIT_EDITOR_COMMAND_BYTES_MAX,
    )
    .with_context(|| {
        format!(
            "run bounded Git editor query in {}",
            repository_root.display()
        )
    })?;
    if !output.status.success() {
        bail!("Git editor query exited with {}", output.status);
    }

    let command = String::from_utf8(output.stdout).context("Git editor command is not UTF-8")?;
    let command = command.trim();
    if command.is_empty() {
        bail!("Git editor command is empty");
    }

    Ok(command.to_owned())
}
