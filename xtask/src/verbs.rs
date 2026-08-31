use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;

pub(crate) mod build;
pub(crate) mod check_structure;
pub(crate) mod desktop_e2e;
pub(crate) mod desktop_release;
pub(crate) mod desktop_scroll;
pub(crate) mod dioxus_web;
pub(crate) mod drift;
pub(crate) mod format;
pub(crate) mod grpc_transport;
pub(crate) mod icon;
pub(crate) mod install;
pub(crate) mod pre_commit;
pub(crate) mod server_highlighting;
pub(crate) mod setup;
pub(crate) mod ship;
pub(crate) mod status_notifier;
pub(crate) mod view_source;
pub(crate) mod wasm_c;

#[derive(Clone, Copy)]
pub(crate) struct Verb(&'static str);

impl Verb {
    pub(crate) const BUILD: Self = Self("build");
    pub(crate) const CHECK_DIOXUS_FORMAT: Self = Self("check-dioxus-format");
    pub(crate) const CHECK_STRUCTURE: Self = Self("check-structure");
    pub(crate) const CHECK_PARSER_WASM: Self = Self("check-parser-wasm");
    pub(crate) const DRIFT_CHECK: Self = Self("drift-check");
    pub(crate) const DESKTOP_SCROLL_BENCHMARK: Self = Self("desktop-scroll-benchmark");
    pub(crate) const DESKTOP_SCROLL_FIXTURE: Self = Self("desktop-scroll-fixture");
    pub(crate) const SERVER_HIGHLIGHTING_BENCHMARK: Self = Self("server-highlighting-benchmark");
    pub(crate) const SERVER_HIGHLIGHTING_PROFILE: Self = Self("server-highlighting-profile");
    pub(crate) const VIEW_SOURCE_BENCHMARK: Self = Self("view-source-benchmark");
    pub(crate) const PRE_COMMIT: Self = Self("pre-commit");
    pub(crate) const GEN_ICON: Self = Self("gen-icon");
    pub(crate) const GRPC_TRANSPORT_BENCHMARK: Self = Self("grpc-transport-benchmark");
    pub(crate) const GRPC_TRANSPORT_SMOKE: Self = Self("grpc-transport-smoke");
    pub(crate) const INSTALL: Self = Self("install");
    pub(crate) const SETUP: Self = Self("setup");
    pub(crate) const SHIP: Self = Self("ship");
    pub(crate) const UNINSTALL: Self = Self("uninstall");
    pub(crate) const WEB_BUILD: Self = Self("web-build");
    pub(crate) const WEB_SERVE: Self = Self("web-serve");
    pub(crate) const WEB_STYLES: Self = Self("web-styles");

    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

pub(crate) fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub(crate) fn cargo_target_directory(root: &Path) -> Result<PathBuf> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .current_dir(root)
        .no_deps()
        .other_options(vec!["--locked".to_string()])
        .exec()
        .context("resolve the Cargo target directory")?;
    Ok(metadata.target_directory.into_std_path_buf())
}

pub(crate) struct WebAssetLock(File);

impl WebAssetLock {
    fn acquire_at(target: &Path) -> Result<Self> {
        let path = target.join("web-assets.lock");
        fs::create_dir_all(path.parent().context("web asset lock parent")?)
            .with_context(|| format!("create lock parent for {}", path.display()))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open web asset lock {}", path.display()))?;
        file.lock()
            .with_context(|| format!("acquire web asset lock {}", path.display()))?;
        Ok(Self(file))
    }
}

impl Drop for WebAssetLock {
    fn drop(&mut self) {
        if let Err(error) = self.0.unlock() {
            eprintln!("failed to release web asset lock: {error}");
        }
    }
}

pub(crate) fn lock_web_assets(root: &Path) -> Result<WebAssetLock> {
    WebAssetLock::acquire_at(&cargo_target_directory(root)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_asset_lock_serializes_release_transactions() {
        let target = tempfile::tempdir().unwrap();
        let guard = WebAssetLock::acquire_at(target.path()).unwrap();
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(target.path().join("web-assets.lock"))
            .unwrap();

        assert!(contender.try_lock().is_err());
        drop(guard);
        contender.lock().unwrap();
        contender.unlock().unwrap();
    }
}
