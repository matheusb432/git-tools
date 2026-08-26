use std::path::Path;

use anyhow::{Context, Result, bail};

pub(super) fn read_command(repository_root: &Path) -> Result<String> {
    let repository = gix::open(repository_root)
        .with_context(|| format!("open Git repository in {}", repository_root.display()))?;
    let command = repository
        .editor()
        .context("Git editor is not configured")?
        .into_string()
        .map_err(|_| anyhow::anyhow!("Git editor command is not UTF-8"))?;
    let command = command.trim();
    if command.is_empty() {
        bail!("Git editor command is empty");
    }

    Ok(command.to_owned())
}
