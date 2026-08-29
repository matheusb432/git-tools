//! Dioxus RSX formatting for `gtl-web`.
//!
//! Check mode compares stdin output because the pinned Dioxus CLI writes source files even when
//! invoked with `--check`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};

use crate::{process, task::Step};

const PACKAGE: &str = "gtl-web";
const SOURCE_DIRECTORIES: &[&str] = &["src", "dev"];
const SOURCE_FILE_COUNT_MAX: usize = 512;

pub(super) fn check_step(directory: &Path) -> Step {
    Step::new("dioxus-rsx-format", "dx", check_arguments()).with_current_directory(directory)
}

fn check_arguments() -> [&'static str; 5] {
    ["fmt", "--check", "--package", PACKAGE, "--locked"]
}

pub(super) fn check() -> Result<()> {
    let web_root = std::env::current_dir()?.join("crates/gtl-web");
    let source_roots = SOURCE_DIRECTORIES
        .iter()
        .map(|directory| web_root.join(directory))
        .collect::<Vec<_>>();
    let files = source_files(&source_roots)?;
    let mut drifted = Vec::new();
    for path in files {
        let source =
            fs::read(&path).with_context(|| format!("read Dioxus source {}", path.display()))?;
        let formatted = process::capture_bytes_with_stdin(
            "dioxus-rsx-format",
            Some(&web_root),
            "dx",
            &["fmt", "--file", "-"],
            &source,
        )?;
        if source != formatted {
            drifted.push(path);
        }
    }

    if drifted.is_empty() {
        return Ok(());
    }
    bail!(
        "dioxus-rsx-format needs formatting in: {}",
        drifted
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn source_files(roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in roots {
        collect_source_files(root, &mut files)?;
    }
    files.sort();
    ensure!(
        !files.is_empty(),
        "Dioxus source inventory is empty: {}",
        roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    ensure!(
        files.len() <= SOURCE_FILE_COUNT_MAX,
        "Dioxus source inventory exceeds {SOURCE_FILE_COUNT_MAX} files"
    );
    Ok(files)
}

fn collect_source_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory).with_context(|| format!("read {}", directory.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("inspect {}", path.display()))?;
        if file_type.is_dir() {
            collect_source_files(&path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            ensure!(
                files.len() < SOURCE_FILE_COUNT_MAX,
                "Dioxus source inventory exceeds {SOURCE_FILE_COUNT_MAX} files"
            );
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_only_the_selected_package_rsx() {
        assert_eq!(
            check_arguments(),
            ["fmt", "--check", "--package", "gtl-web", "--locked"]
        );
        assert_eq!(SOURCE_DIRECTORIES, ["src", "dev"]);
    }
}
