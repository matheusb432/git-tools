//! Labeled command steps and run orchestration.
//!
//! A [`Step`] is one child process to spawn; [`run_all`] executes a plan and bails on the first
//! failure.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::process;

/// One labeled child process: `program arguments…` with optional environment additions.
pub(crate) struct Step {
    label: String,
    program: String,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    removed_environment: Vec<String>,
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
            removed_environment: Vec::new(),
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

    /// Remove inherited environment variables before the step spawns.
    pub(crate) fn without_environment(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.removed_environment
            .extend(names.into_iter().map(Into::into));
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

    pub(crate) fn removed_environment(&self) -> &[String] {
        &self.removed_environment
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
