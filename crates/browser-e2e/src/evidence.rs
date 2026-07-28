use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use playwright_rs::protocol::{Animations, Caret, Page, ScreenshotOptions};

use crate::browser::{Session, operation};

const EVIDENCE_ENVIRONMENT_VARIABLE: &str = "TEST_EVIDENCES_OUTPUT_PATH";

pub struct Recording {
    page: Page,
    name: &'static str,
}

impl Recording {
    #[must_use]
    pub fn new(session: &Session, name: &'static str) -> Self {
        Self {
            page: session.page.clone(),
            name,
        }
    }

    pub async fn finish(self, passed: bool) -> Result<Vec<PathBuf>> {
        let Some(root) = std::env::var_os(EVIDENCE_ENVIRONMENT_VARIABLE) else {
            return Ok(Vec::new());
        };
        let screenshot = evidence_path(
            Path::new(&root),
            if passed { "success" } else { "fail" },
            self.name,
        );
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

    use super::evidence_path;

    #[test]
    fn evidence_path_uses_the_supplied_root_and_stable_suite_name() {
        let root = Path::new("/evidence");
        assert_eq!(
            evidence_path(root, "success", "offline-artifact-fold-all"),
            root.join("success/chromium/offline-artifact-fold-all.png")
        );
    }
}
