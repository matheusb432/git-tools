use std::time::Duration;

use anyhow::{Context, Result};
use thirtyfour::{By, WebDriver, WebElement, prelude::ElementQueryable as _};

use super::wait::ASSERTION_TIMEOUT;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub async fn by_aria_label(driver: &WebDriver, expected: &str) -> Result<WebElement> {
    let selector = format!(r#"[aria-label="{}"]"#, css_string(expected));
    driver
        .query(By::Css(&selector))
        .ignore_errors(true)
        .and_displayed()
        .wait(ASSERTION_TIMEOUT, POLL_INTERVAL)
        .first()
        .await
        .with_context(|| format!("find visible element labelled {expected:?}"))
}

pub async fn by_aria_label_containing(
    driver: &WebDriver,
    prefix: &str,
    contained: &str,
) -> Result<WebElement> {
    let selector = format!(
        r#"[aria-label^="{}"][aria-label*="{}"]"#,
        css_string(prefix),
        css_string(contained)
    );
    driver
        .query(By::Css(&selector))
        .ignore_errors(true)
        .and_displayed()
        .wait(ASSERTION_TIMEOUT, POLL_INTERVAL)
        .first()
        .await
        .with_context(|| {
            format!("find visible element labelled {prefix:?} containing {contained:?}")
        })
}

fn css_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
