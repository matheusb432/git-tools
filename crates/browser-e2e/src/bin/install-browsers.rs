use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, Result, bail};

fn main() -> Result<()> {
    let cache_root = env::var_os("PLAYWRIGHT_DRIVER_CACHE_DIR")
        .map(PathBuf::from)
        .context("PLAYWRIGHT_DRIVER_CACHE_DIR is required")?;
    let driver = driver_paths(&cache_root)?;
    let status = install_command(&driver)
        .status()
        .with_context(|| format!("run Playwright driver {}", driver.node.display()))?;
    if !status.success() {
        bail!(
            "install cached Playwright Chromium failed (exit {})",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

struct DriverPaths {
    node: PathBuf,
    cli: PathBuf,
}

fn install_command(driver: &DriverPaths) -> Command {
    let mut command = Command::new(&driver.node);
    command.arg(&driver.cli).args(["install", "chromium"]);
    command
}

fn driver_paths(cache_root: &Path) -> Result<DriverPaths> {
    let platform = driver_platform()?;
    let driver = cache_root.join(format!(
        "playwright-{}-{platform}",
        playwright_rs::PLAYWRIGHT_VERSION
    ));
    let node = driver.join(format!("node{}", env::consts::EXE_SUFFIX));
    let cli = driver.join("package/cli.js");
    if !node.is_file() || !cli.is_file() {
        bail!(
            "Playwright driver is incomplete at {}; expected {} and {}",
            driver.display(),
            node.display(),
            cli.display()
        );
    }
    Ok(DriverPaths { node, cli })
}

fn driver_platform() -> Result<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => Ok("linux"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("windows", "x86_64") => Ok("win32_x64"),
        ("windows", "aarch64") => Ok("win32_arm64"),
        (os, arch) => bail!("unsupported Playwright driver platform: {os} {arch}"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{driver_paths, driver_platform, install_command};

    #[test]
    fn driver_paths_select_the_pinned_current_platform() {
        let cache = tempfile::tempdir().unwrap();
        let driver = cache.path().join(format!(
            "playwright-{}-{}",
            playwright_rs::PLAYWRIGHT_VERSION,
            driver_platform().unwrap()
        ));
        fs::create_dir_all(driver.join("package")).unwrap();
        fs::write(
            driver.join(format!("node{}", std::env::consts::EXE_SUFFIX)),
            [],
        )
        .unwrap();
        fs::write(driver.join("package/cli.js"), []).unwrap();

        let paths = driver_paths(cache.path()).unwrap();

        assert_eq!(
            paths.node,
            driver.join(format!("node{}", std::env::consts::EXE_SUFFIX))
        );
        assert_eq!(paths.cli, driver.join("package/cli.js"));
    }

    #[test]
    fn incomplete_driver_is_rejected_before_installation() {
        let cache = tempfile::tempdir().unwrap();

        let error = driver_paths(cache.path()).err().unwrap();

        assert!(error.to_string().contains("driver is incomplete"));
    }

    #[test]
    fn browser_install_never_requests_machine_dependencies() {
        let cache = tempfile::tempdir().unwrap();
        let driver = cache.path().join(format!(
            "playwright-{}-{}",
            playwright_rs::PLAYWRIGHT_VERSION,
            driver_platform().unwrap()
        ));
        fs::create_dir_all(driver.join("package")).unwrap();
        fs::write(
            driver.join(format!("node{}", std::env::consts::EXE_SUFFIX)),
            [],
        )
        .unwrap();
        fs::write(driver.join("package/cli.js"), []).unwrap();
        let paths = driver_paths(cache.path()).unwrap();

        let command = install_command(&paths);
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            arguments,
            [
                paths.cli.to_string_lossy().into_owned(),
                "install".to_owned(),
                "chromium".to_owned()
            ]
        );
        assert!(!arguments.iter().any(|argument| argument == "--with-deps"));
    }
}
