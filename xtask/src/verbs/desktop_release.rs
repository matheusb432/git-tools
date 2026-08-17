//! Production Tauri configuration for cargo-driven desktop builds.

use std::{fs, path::Path};

use anyhow::{Context, Result};

use super::{dioxus_web, lock_web_assets, repository_root};
use crate::{process, task::Step};

const PRODUCTION_CONFIG_PATH: &str = "crates/gtl-desktop/tauri.production.conf.json";
pub(crate) const PRODUCTION_FEATURES: &str = "custom-protocol";

pub(crate) fn run_cargo(label: &str, arguments: &[&str]) -> Result<()> {
    let root = repository_root();
    let _lock = lock_web_assets(&root)?;
    run_cargo_unlocked(label, arguments, &root)
}

pub(crate) fn run_cargo_unlocked(label: &str, arguments: &[&str], root: &Path) -> Result<()> {
    dioxus_web::verify_staged_bundle(root)?;
    let config = load_production_config(root)?;
    process::run_step(&production_cargo_step(label, arguments, root, config))
}

fn production_cargo_step(label: &str, arguments: &[&str], root: &Path, config: String) -> Step {
    Step::new(label, "cargo", arguments.iter().copied())
        .with_environment("TAURI_CONFIG", config)
        .with_current_directory(root)
}

fn load_production_config(root: &Path) -> Result<String> {
    let path = root.join(PRODUCTION_CONFIG_PATH);
    fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn production_config_load_reports_its_source_path() {
        let root = TempDir::new().expect("temporary repository");
        let error = load_production_config(root.path())
            .expect_err("missing production config must fail")
            .to_string();

        assert!(error.contains(PRODUCTION_CONFIG_PATH), "{error}");
    }

    #[test]
    fn production_config_load_preserves_the_overlay() {
        let root = TempDir::new().expect("temporary repository");
        let path = root.path().join(PRODUCTION_CONFIG_PATH);
        fs::create_dir_all(path.parent().expect("config parent"))
            .expect("config parent is writable");
        fs::write(&path, "{\"build\":{}}\n").expect("config is writable");

        assert_eq!(
            load_production_config(root.path()).expect("config loads"),
            "{\"build\":{}}\n"
        );
    }

    #[test]
    fn production_cargo_step_injects_the_overlay_for_native_and_xwin_arguments() {
        let root = Path::new("repository-root");
        let config = "{\"build\":{\"frontendDist\":\"dist\"}}\n";
        let step = production_cargo_step(
            "viewer release",
            &["xwin", "build", "-p", "gtl-desktop"],
            root,
            config.to_string(),
        );

        assert_eq!(step.label(), "viewer release");
        assert_eq!(step.program(), "cargo");
        assert_eq!(step.arguments(), ["xwin", "build", "-p", "gtl-desktop"]);
        assert_eq!(step.current_directory(), Some(root));
        assert_eq!(
            step.environment(),
            [("TAURI_CONFIG".to_string(), config.to_string())]
        );
    }
}
