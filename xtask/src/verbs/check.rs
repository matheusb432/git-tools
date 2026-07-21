//! Aggregate read-only quality gate: formatting drift, then every linter.

use anyhow::Result;

use super::{format, lint};
use crate::{
    process::{self, Status},
    task,
    verb::Verb,
};

/// Run the complete read-only gate: verify formatting, then run every linter.
pub(crate) fn run() -> Result<()> {
    task::check_all(&format::check_steps(false)?, "run `just fmt`")?;
    lint::linters()?;
    process::result(Verb::CHECK, Status::Pass);
    Ok(())
}
