use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use playwright_rs::protocol::{Animations, Caret, Page, ScreenshotOptions};

use crate::browser::{Session, operation};

pub const EVIDENCE_OUTPUT_PATH_ENVIRONMENT_VARIABLE: &str = "GTL_E2E_EVIDENCE_OUTPUT_PATH";
const EVIDENCES_OUTPUT_PATH_ENVIRONMENT_VARIABLE: &str = "TEST_EVIDENCES_OUTPUT_PATH";

pub struct Recording {
    page: Page,
    name: String,
}

impl Recording {
    #[must_use]
    pub fn new(session: &Session, name: &str) -> Self {
        Self {
            page: session.page.clone(),
            name: name.to_owned(),
        }
    }

    pub async fn finish(self, passed: bool) -> Result<Vec<PathBuf>> {
        if !should_capture(
            passed,
            env::var_os(EVIDENCES_OUTPUT_PATH_ENVIRONMENT_VARIABLE).is_some(),
        ) {
            return Ok(Vec::new());
        }
        let root = env::var_os(EVIDENCE_OUTPUT_PATH_ENVIRONMENT_VARIABLE)
            .map(PathBuf::from)
            .context("GTL browser E2E evidence output path is missing")?;
        let screenshot = evidence_path(&root, if passed { "success" } else { "fail" }, &self.name);
        let parent = screenshot
            .parent()
            .context("evidence screenshot has no parent")?;
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create evidence directory {}", parent.display()))?;
        operation("write browser evidence screenshot", async {
            self.page
                .screenshot_to_file(&screenshot, stable_screenshot())
                .await
                .with_context(|| format!("write evidence screenshot {}", screenshot.display()))
        })
        .await?;
        Ok(vec![screenshot])
    }
}

fn should_capture(passed: bool, success_evidence_requested: bool) -> bool {
    !passed || success_evidence_requested
}

pub fn evidence_path(root: &Path, outcome: &str, name: &str) -> PathBuf {
    root.join(outcome)
        .join("chromium")
        .join(format!("{name}.png"))
}

fn stable_screenshot() -> ScreenshotOptions {
    ScreenshotOptions::builder()
        .animations(Animations::Disabled)
        .caret(Caret::Hide)
        .build()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{evidence_path, should_capture};

    #[test]
    fn evidence_path_uses_the_supplied_root_and_stable_suite_name() {
        let root = Path::new("/evidence");
        assert_eq!(
            evidence_path(root, "success", "raw-artifact-lifecycle"),
            root.join("success/chromium/raw-artifact-lifecycle.png")
        );
    }

    #[test]
    fn failures_capture_without_requesting_success_evidence() {
        assert!(should_capture(false, false));
        assert!(!should_capture(true, false));
        assert!(should_capture(true, true));
    }
}
