use std::time::Duration;

use anyhow::{Context, Result};
use gtl_web_contracts::test_ids::TestId;
use thirtyfour::{By, WebDriver, WebElement, prelude::ElementQueryable as _};

use super::wait::ASSERTION_TIMEOUT;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub async fn by_test_id(driver: &WebDriver, test_id: TestId) -> Result<WebElement> {
    driver
        .query(By::Css(test_id.selector()))
        .ignore_errors(true)
        .and_displayed()
        .wait(ASSERTION_TIMEOUT, POLL_INTERVAL)
        .first()
        .await
        .with_context(|| format!("find visible element with test ID {:?}", test_id.value()))
}
