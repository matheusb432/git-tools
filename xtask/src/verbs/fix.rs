//! Autofix verb — apply machine-applicable lint fixes, then reformat.

use anyhow::Result;

use super::{format, lint};
use crate::{
    process::{self, Status},
    task,
    verb::Verb,
};

/// Apply autofixable Clippy and Oxlint fixes, then run every formatter in place.
pub(crate) fn run(clippy_extra: &[String]) -> Result<()> {
    lint::fix(clippy_extra)?;
    task::run_all(&format::write_steps(false)?)?;
    process::result(Verb::FIX, Status::Done);
    Ok(())
}
