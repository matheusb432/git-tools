//! Labeled command steps and run/check orchestration.
//!
//! A [`Step`] is one child process to spawn; [`run_all`] executes a plan and bails on the first
//! failure, while [`check_all`] runs every step and reports the complete set that failed. The
//! quality verbs build their plans as `Step` vectors so the read-only gate can name every drifted
//! or failing tool at once.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::process;

/// One labeled child process: `program arguments…` with optional environment additions.
pub(crate) struct Step {
    label: String,
    program: String,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    current_directory: Option<PathBuf>,
}

impl Step {
    pub(crate) fn new(
        label: impl Into<String>,
        program: impl Into<String>,
        arguments: impl IntoIterator<Item: Into<String>>,
    ) -> Self {
        Self {
            label: label.into(),
            program: program.into(),
            arguments: arguments.into_iter().map(Into::into).collect(),
            environment: Vec::new(),
            current_directory: None,
        }
    }

    /// Append more arguments (e.g. a variable file list) to an existing step.
    pub(crate) fn with_arguments(
        mut self,
        arguments_extra: impl IntoIterator<Item: Into<String>>,
    ) -> Self {
        self.arguments
            .extend(arguments_extra.into_iter().map(Into::into));
        self
    }

    /// Set an environment variable applied when the step spawns.
    pub(crate) fn with_environment(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.environment.push((key.into(), value.into()));
        self
    }

    pub(crate) fn with_current_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.current_directory = Some(directory.into());
        self
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn program(&self) -> &str {
        &self.program
    }

    pub(crate) fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(crate) fn environment(&self) -> &[(String, String)] {
        &self.environment
    }

    pub(crate) fn current_directory(&self) -> Option<&Path> {
        self.current_directory.as_deref()
    }
}

/// Run every step in order, bailing on the first failure.
pub(crate) fn run_all(steps: &[Step]) -> Result<()> {
    for step in steps {
        process::run_step(step)?;
    }
    Ok(())
}

/// Run every step, then bail listing all that failed with the shared `remediation` hint.
pub(crate) fn check_all(steps: &[Step], remediation: &str) -> Result<()> {
    let outcomes = steps
        .iter()
        .map(|step| process::step_succeeds(step).map(|success| (step.label(), success)))
        .collect::<Result<Vec<_>>>()?;
    if let Some(message) = failure_message(&outcomes, remediation) {
        bail!(message);
    }
    Ok(())
}

fn failure_message(outcomes: &[(&str, bool)], remediation: &str) -> Option<String> {
    let failed = outcomes
        .iter()
        .filter_map(|(label, success)| (!*success).then_some(*label))
        .collect::<Vec<_>>();
    if failed.is_empty() {
        None
    } else {
        Some(format!(
            "check failed in: {} — {remediation}",
            failed.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_message_lists_failed_steps() {
        let outcomes = [("cargo-fmt", false), ("taplo", true), ("clippy", false)];
        assert_eq!(
            failure_message(&outcomes, "run `just fix`").as_deref(),
            Some("check failed in: cargo-fmt, clippy — run `just fix`")
        );
        assert_eq!(
            failure_message(&[("cargo-fmt", true)], "run `just fix`"),
            None
        );
    }
}
