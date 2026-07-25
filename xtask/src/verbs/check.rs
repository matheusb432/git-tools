//! Aggregate read-only quality gate.

use anyhow::Result;

use super::{ast_grep, format, lint};
use crate::{
    process::{self, Status},
    task,
    verb::Verb,
};

/// Verifies formatting, runs the lint sweep, then checks configured ast-grep rules.
pub(crate) fn run() -> Result<()> {
    task::check_all(&format::check_steps(false)?, "run `just fmt`")?;
    lint::linters()?;
    if let Some(step) = ast_grep::check_step() {
        task::check_all(&[step], "fix the reported ast-grep findings")?;
    }
    process::result(Verb::CHECK, Status::Pass);
    Ok(())
}
