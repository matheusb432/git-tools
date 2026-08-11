use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct CheckPaths {
    pub(super) rust: Vec<PathBuf>,
    pub(super) dioxus: Vec<PathBuf>,
    pub(super) toml: Vec<PathBuf>,
    pub(super) markdown: Vec<PathBuf>,
}

impl CheckPaths {
    pub(super) fn classify(paths: &[PathBuf]) -> Self {
        let mut checks = Self::default();
        for path in paths {
            match path.extension().and_then(|extension| extension.to_str()) {
                Some("rs") => checks.rust.push(path.clone()),
                Some("toml") => checks.toml.push(path.clone()),
                Some("md") => checks.markdown.push(path.clone()),
                _ => {}
            }
            if is_dioxus_source_path(path)
                && path.extension().is_some_and(|extension| extension == "rs")
            {
                checks.dioxus.push(path.clone());
            }
        }
        checks
    }
}

fn is_dioxus_source_path(path: &Path) -> bool {
    path.starts_with(Path::new("crates/gtl-web/src"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::CheckPaths;

    fn paths(values: &[&str]) -> Vec<PathBuf> {
        values.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn groups_repository_formatter_and_linter_ownership() {
        let classified = CheckPaths::classify(&paths(&[
            "crates/gtl-models/src/lib.rs",
            "crates/gtl-web/src/app.rs",
            "Cargo.toml",
            "docs/Guide Name.md",
            "crates/gtl-artifacts/src/styles/base.css",
            ".github/workflows/check.yaml",
        ]));

        assert_eq!(
            classified.rust,
            paths(&["crates/gtl-models/src/lib.rs", "crates/gtl-web/src/app.rs"])
        );
        assert_eq!(classified.dioxus, paths(&["crates/gtl-web/src/app.rs"]));
        assert_eq!(classified.toml, paths(&["Cargo.toml"]));
        assert_eq!(classified.markdown, paths(&["docs/Guide Name.md"]));
    }
}
