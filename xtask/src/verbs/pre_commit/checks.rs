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
            checks.classify_path(path);
        }
        checks
    }

    fn classify_path(&mut self, path: &Path) {
        let extension = path.extension().and_then(|extension| extension.to_str());
        match extension {
            Some("rs") => self.rust.push(path.to_path_buf()),
            Some("toml") => self.toml.push(path.to_path_buf()),
            Some("md") => self.markdown.push(path.to_path_buf()),
            _ => {}
        }
        if extension == Some("rs") && is_dioxus_source_path(path) {
            self.dioxus.push(path.to_path_buf());
        }
    }
}

fn is_dioxus_source_path(path: &Path) -> bool {
    path.starts_with(Path::new("crates/gtl-web/src"))
        || path.starts_with(Path::new("crates/gtl-web/dev"))
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
            "crates/gtl-web/dev/stories/button.rs",
            "Cargo.toml",
            "docs/Guide Name.md",
            "crates/gtl-artifacts/src/styles/base.css",
            ".github/workflows/check.yaml",
        ]));

        assert_eq!(
            classified.rust,
            paths(&[
                "crates/gtl-models/src/lib.rs",
                "crates/gtl-web/src/app.rs",
                "crates/gtl-web/dev/stories/button.rs",
            ])
        );
        assert_eq!(
            classified.dioxus,
            paths(&[
                "crates/gtl-web/src/app.rs",
                "crates/gtl-web/dev/stories/button.rs",
            ])
        );
        assert_eq!(classified.toml, paths(&["Cargo.toml"]));
        assert_eq!(classified.markdown, paths(&["docs/Guide Name.md"]));
    }
}
