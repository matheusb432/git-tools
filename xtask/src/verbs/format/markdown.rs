//! Markdown formatting plan (rumdl over tracked files).

use std::path::{Path, PathBuf};

use crate::task::Step;

pub(crate) fn check_step_for_files(files: Vec<PathBuf>, directory: &Path) -> Step {
    Step::new("rumdl", "rumdl", ["fmt", "--check"])
        .with_arguments(
            files
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned()),
        )
        .with_current_directory(directory)
}
