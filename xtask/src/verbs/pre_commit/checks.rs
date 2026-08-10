use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct CheckPaths {
    pub(super) rust: Vec<PathBuf>,
    pub(super) dioxus: Vec<PathBuf>,
    pub(super) toml: Vec<PathBuf>,
    pub(super) markdown: Vec<PathBuf>,
    pub(super) frontend_format: Vec<PathBuf>,
    pub(super) frontend_lint: Vec<PathBuf>,
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
            if is_frontend_format_path(path) {
                checks.frontend_format.push(path.clone());
            }
            if is_frontend_lint_path(path) {
                checks.frontend_lint.push(path.clone());
            }
        }
        checks
    }
}

fn is_dioxus_source_path(path: &Path) -> bool {
    path.starts_with(Path::new("crates/gtl-web/src"))
}

fn is_frontend_format_path(path: &Path) -> bool {
    const ROOT_PATHS: &[&str] = &[
        ".oxfmtrc.json",
        ".oxlintrc.json",
        "package.json",
        "tsconfig.json",
        "vitest.config.mjs",
    ];
    ROOT_PATHS
        .iter()
        .any(|candidate| path == Path::new(candidate))
        || (is_frontend_source_path(path, &["boot", "diff", "diff-island", "shared"])
            && extension_matches(
                path,
                &["js", "jsx", "ts", "tsx", "mjs", "cjs", "json", "jsonc"],
            ))
}

fn is_frontend_lint_path(path: &Path) -> bool {
    is_frontend_source_path(path, &["boot", "diff", "diff-island", "shared"])
        && extension_matches(path, &["js", "jsx", "ts", "tsx", "mjs", "cjs"])
}

fn is_frontend_source_path(path: &Path, directories: &[&str]) -> bool {
    directories
        .iter()
        .any(|directory| path.starts_with(Path::new("frontend").join(directory)))
}

fn extension_matches(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extensions.contains(&extension))
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
            "frontend/shared/wheel.ts",
            "frontend/diff/vite.config.mjs",
            "frontend/boot/theme-boot.ts",
            "frontend/diff-island/adapter.ts",
            ".oxfmtrc.json",
            ".oxlintrc.json",
            "package.json",
            "tsconfig.json",
            "vitest.config.mjs",
            "frontend/test/dom-stub.ts",
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
        assert_eq!(
            classified.frontend_format,
            paths(&[
                "frontend/shared/wheel.ts",
                "frontend/diff/vite.config.mjs",
                "frontend/boot/theme-boot.ts",
                "frontend/diff-island/adapter.ts",
                ".oxfmtrc.json",
                ".oxlintrc.json",
                "package.json",
                "tsconfig.json",
                "vitest.config.mjs",
            ])
        );
        assert_eq!(
            classified.frontend_lint,
            paths(&[
                "frontend/shared/wheel.ts",
                "frontend/diff/vite.config.mjs",
                "frontend/boot/theme-boot.ts",
                "frontend/diff-island/adapter.ts",
            ])
        );
    }
}
