use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use thirtyfour::WebDriver;

use super::wait::{self, WEBDRIVER_OPERATION_TIMEOUT};

const EVIDENCES_OUTPUT_PATH_ENV: &str = "TEST_EVIDENCES_OUTPUT_PATH";

pub async fn capture(driver: &WebDriver, name: &str, outcome: &str) -> Result<()> {
    let Some(root) = env::var_os(EVIDENCES_OUTPUT_PATH_ENV) else {
        return Ok(());
    };
    let path = evidence_path(&PathBuf::from(root), outcome, "thirtyfour", name);
    let directory = path.parent().expect("evidence path has a parent directory");
    std::fs::create_dir_all(directory)
        .with_context(|| format!("create evidence directory {}", directory.display()))?;
    wait::within(
        "disable animations before evidence capture",
        WEBDRIVER_OPERATION_TIMEOUT,
        async {
            driver
                .execute(
            r#"
const id = "desktop-e2e-stable-capture";
if (!document.getElementById(id)) {
  const style = document.createElement("style");
  style.id = id;
  style.textContent = "*, *::before, *::after { animation: none !important; transition: none !important; caret-color: transparent !important; }";
  document.head.appendChild(style);
}
"#,
                    Vec::new(),
                )
                .await
                .context("disable animations before evidence capture")
        },
    )
    .await?;
    wait::within(
        &format!("capture evidence {}", path.display()),
        WEBDRIVER_OPERATION_TIMEOUT,
        async { driver.screenshot(&path).await.map_err(anyhow::Error::from) },
    )
    .await
}

pub fn evidence_path(root: &Path, outcome: &str, suite: &str, name: &str) -> PathBuf {
    root.join(outcome).join(suite).join(format!("{name}.png"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::evidence_path;

    #[test]
    fn evidence_path_uses_the_supplied_root_and_stable_suite_name() {
        let root = Path::new("/evidence");
        assert_eq!(
            evidence_path(root, "success", "thirtyfour", "viewer-live-lifecycle"),
            root.join("success/thirtyfour/viewer-live-lifecycle.png")
        );
    }
}
