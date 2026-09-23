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
pub(crate) mod dioxus_format;
pub(crate) mod dioxus_web;
pub(crate) mod icon;
pub(crate) mod install;
pub(crate) mod install_path;
pub(crate) mod macos_package;
pub(crate) mod pre_commit;
pub(crate) mod server_highlighting;
pub(crate) mod ship;
pub(crate) mod status_notifier;
pub(crate) mod view_source;
pub(crate) mod wasm_c;

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
