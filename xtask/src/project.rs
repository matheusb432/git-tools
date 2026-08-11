//! Repository-owned test declarations.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use sample_project::{Test, surface};

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
        let path = target.join("xtask/web-assets.lock");
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

pub(crate) struct TestDeclaration {
    #[cfg(test)]
    label: &'static str,
    test: Test,
}

impl TestDeclaration {
    fn new(label: &'static str, test: Test) -> Self {
        #[cfg(not(test))]
        let _ = label;
        Self {
            #[cfg(test)]
            label,
            test,
        }
    }

    #[cfg(test)]
    pub(crate) fn label(&self) -> &'static str {
        self.label
    }

    pub(crate) fn into_test(self) -> Test {
        self.test
    }
}

pub(crate) fn tests_unit() -> Result<Vec<TestDeclaration>> {
    Ok(vec![
        TestDeclaration::new(
            "unit",
            Test::try_new("unit", surface::CARGO, "cargo")?
                .args(["test", "--quiet"])
                .verbose_arguments(["--", "--nocapture"]),
        ),
        parser_all_features()?,
        web_desktop()?,
        web_artifact()?,
    ])
}

pub(crate) fn tests_e2e(executable: OsString) -> Result<Vec<TestDeclaration>> {
    Ok(vec![cli_e2e()?, desktop_e2e(executable)?])
}

pub(crate) fn tests_all(executable: OsString) -> Result<Vec<TestDeclaration>> {
    Ok(vec![
        TestDeclaration::new(
            "unit",
            Test::try_new("unit", surface::CARGO, "cargo")?
                .args(["test", "--workspace", "--quiet"])
                .verbose_arguments(["--", "--nocapture"]),
        ),
        parser_all_features()?,
        web_artifact()?,
        worker("drift", &executable, "drift-check")?,
        cli_e2e()?,
        desktop_e2e(executable)?,
    ])
}

fn parser_all_features() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "parser-all-features",
        Test::try_new("parser-all-features", surface::CARGO, "cargo")?.args([
            "test",
            "--locked",
            "--quiet",
            "-p",
            "gtl-parser",
            "--all-features",
        ]),
    ))
}

fn web_desktop() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "web-desktop",
        Test::try_new("web-desktop", surface::CARGO, "cargo")?
            .args(["test", "--locked", "--quiet", "-p", "gtl-web"]),
    ))
}

fn web_artifact() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "web-artifact",
        Test::try_new("web-artifact", surface::CARGO, "cargo")?.args([
            "test",
            "--locked",
            "--quiet",
            "-p",
            "gtl-web",
            "--no-default-features",
            "--features",
            "artifact",
        ]),
    ))
}

fn worker(
    label: &'static str,
    executable: &OsString,
    verb: &'static str,
) -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        label,
        Test::try_new(label, surface::OPAQUE, executable.clone())?.arg(verb),
    ))
}

fn desktop_e2e(executable: OsString) -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "desktop-e2e",
        Test::try_new("desktop-e2e", surface::OPAQUE, executable)?
            .arg("desktop-e2e-worker")
            .verbose_arguments(["--verbose"])
            .accepts_evidences()
            .timeout(Duration::from_hours(2)),
    ))
}

fn cli_e2e() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "cli-e2e",
        Test::try_new("cli-e2e", surface::CARGO, "cargo")?
            .args([
                "test",
                "-p",
                "gtl-cli-e2e",
                "--features",
                "e2e",
                "--test",
                "daemon_lifecycle",
                "--",
                "--test-threads",
                "1",
            ])
            .verbose_arguments(["--nocapture"])
            .timeout(Duration::from_mins(5)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_asset_lock_serializes_release_transactions() {
        let target = tempfile::tempdir().expect("temporary target directory");
        let guard = WebAssetLock::acquire_at(target.path()).expect("first lock is acquired");
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(target.path().join("xtask/web-assets.lock"))
            .expect("lock contender opens");

        assert!(contender.try_lock().is_err());
        drop(guard);
        contender.lock().expect("contender acquires after release");
        contender.unlock().expect("contender releases");
    }
}
