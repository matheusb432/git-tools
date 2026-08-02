use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result, bail};

use super::{HostCargoEnvironment, IsolatedEnv, Sandbox};

mod command;

const INSTALL_ATTEMPTS_MAX: u32 = 4;
const DRIVER_CACHE_DIR: &str = ".cache/playwright-driver";
const BROWSER_CACHE_DIR: &str = "browsers";
const INSTALL_TIMEOUT: Duration = Duration::from_mins(3);
const TEST_TIMEOUT: Duration = Duration::from_mins(2);

struct CachePaths {
    driver: PathBuf,
    browsers: PathBuf,
}

fn cache_paths(repository_root: &Path) -> CachePaths {
    let driver = repository_root.join(DRIVER_CACHE_DIR);
    let browsers = driver.join(BROWSER_CACHE_DIR);
    CachePaths { driver, browsers }
}

pub(super) fn run(
    sandbox: &Sandbox,
    environment: &IsolatedEnv,
    host_environment: &HostCargoEnvironment,
) -> Result<()> {
    let repository_root = std::env::current_dir().context("resolve repository root")?;
    let cache = cache_paths(&repository_root);
    fs::create_dir_all(&cache.driver)
        .with_context(|| format!("create Playwright driver cache {}", cache.driver.display()))?;
    fs::create_dir_all(&cache.browsers).with_context(|| {
        format!(
            "create Playwright browser cache {}",
            cache.browsers.display()
        )
    })?;

    let mut environment = environment.clone();
    environment.set("PLAYWRIGHT_DRIVER_CACHE_DIR", &cache.driver);
    environment.set("PLAYWRIGHT_BROWSERS_PATH", &cache.browsers);
    environment.set("GTL_E2E_CLI_BINARY", &sandbox.cli_binary);

    install_browsers(sandbox, &environment, host_environment)?;
    command::run(
        sandbox,
        &environment,
        host_environment,
        "playwright-artifact.log",
        &[
            "test",
            "-p",
            "gtl-browser-e2e",
            "--features",
            "e2e",
            "--test",
            "offline_artifact",
            "--",
            "--test-threads",
            "1",
        ],
        "run Playwright offline artifact E2E",
        TEST_TIMEOUT,
    )
}

fn install_browsers(
    sandbox: &Sandbox,
    environment: &IsolatedEnv,
    host_environment: &HostCargoEnvironment,
) -> Result<()> {
    let mut last_error = None;
    for attempt in 1..=INSTALL_ATTEMPTS_MAX {
        match command::run(
            sandbox,
            environment,
            host_environment,
            "playwright-install.log",
            &[
                "run",
                "--quiet",
                "-p",
                "gtl-browser-e2e",
                "--features",
                "e2e",
                "--bin",
                "install-browsers",
            ],
            "install Chromium for Playwright E2E",
            INSTALL_TIMEOUT,
        ) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some((attempt, error)),
        }
    }
    let (attempt, error) = last_error.context("Playwright installer made no attempts")?;
    bail!("install Chromium for Playwright E2E failed after {attempt} attempts: {error:#}")
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum InstallOutcome {
    Installed { attempt: u32 },
    Failed,
}

#[cfg(test)]
fn decide_install(install: &impl Fn() -> bool, attempts_max: u32) -> InstallOutcome {
    for attempt in 1..=attempts_max {
        if install() {
            return InstallOutcome::Installed { attempt };
        }
    }
    InstallOutcome::Failed
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, path::Path};

    use super::{InstallOutcome, cache_paths, decide_install};

    #[test]
    fn browser_assets_share_the_repository_playwright_cache_boundary() {
        let cache = cache_paths(Path::new("/repository"));

        assert_eq!(
            cache.driver,
            Path::new("/repository/.cache/playwright-driver")
        );
        assert_eq!(
            cache.browsers,
            Path::new("/repository/.cache/playwright-driver/browsers")
        );
    }

    #[test]
    fn install_stops_on_the_first_success() {
        let calls = Cell::new(0);
        let outcome = decide_install(
            &|| {
                calls.set(calls.get() + 1);
                calls.get() == 2
            },
            4,
        );
        assert_eq!(outcome, InstallOutcome::Installed { attempt: 2 });
    }

    #[test]
    fn install_stops_at_four_attempts() {
        let calls = Cell::new(0);
        assert_eq!(
            decide_install(
                &|| {
                    calls.set(calls.get() + 1);
                    false
                },
                4
            ),
            InstallOutcome::Failed
        );
        assert_eq!(calls.get(), 4);
    }
}
