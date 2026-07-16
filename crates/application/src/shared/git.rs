//! Small combinators over the [`GitRunner`](crate::ports::GitRunner) port shared
//! by the local-git command flows and slices.

use std::path::Path;

use crate::ports::GitRunner;

/// Runs git and returns trimmed stdout on a clean exit, else `None`.
pub fn capture(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> Option<String> {
    match runner.run(repo, args) {
        Ok(out) if out.exit_code == 0 => Some(out.stdout.trim().to_string()),
        _ => None,
    }
}

/// True when `<onto>` exists as a local branch.
pub fn onto_exists(runner: &impl GitRunner, repo: &Path, onto: &str) -> bool {
    matches!(
        runner.run(repo, &["rev-parse", "--verify", &format!("refs/heads/{onto}")]),
        Ok(out) if out.exit_code == 0
    )
}
