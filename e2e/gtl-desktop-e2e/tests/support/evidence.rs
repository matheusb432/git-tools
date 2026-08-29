use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use thirtyfour::WebDriver;

use super::wait::{self, WEBDRIVER_OPERATION_TIMEOUT};

const EVIDENCES_OUTPUT_PATH_ENV: &str = "TEST_EVIDENCES_OUTPUT_PATH";
const EVIDENCE_OUTPUT_PATH_ENV: &str = "GTL_E2E_EVIDENCE_OUTPUT_PATH";

pub async fn capture(driver: &WebDriver, name: &str, passed: bool) -> Result<Option<PathBuf>> {
    if !should_capture(passed, env::var_os(EVIDENCES_OUTPUT_PATH_ENV).is_some()) {
        return Ok(None);
    }
    let root = env::var_os(EVIDENCE_OUTPUT_PATH_ENV)
        .map(PathBuf::from)
        .context("viewer E2E evidence output path is missing")?;
    let outcome = if passed { "success" } else { "fail" };
    let path = evidence_path(&root, outcome, "thirtyfour", name);
    let directory = path
        .parent()
        .context("evidence path has no parent directory")?;
    std::fs::create_dir_all(directory)
        .with_context(|| format!("create evidence directory {}", directory.display()))?;
    wait::within(
        &format!("capture evidence {}", path.display()),
        WEBDRIVER_OPERATION_TIMEOUT,
        async { driver.screenshot(&path).await.map_err(anyhow::Error::from) },
    )
    .await?;
    Ok(Some(path))
}

fn should_capture(passed: bool, success_evidence_requested: bool) -> bool {
    !passed || success_evidence_requested
}

pub fn evidence_path(root: &Path, outcome: &str, suite: &str, name: &str) -> PathBuf {
    root.join(outcome).join(suite).join(format!("{name}.png"))
}
