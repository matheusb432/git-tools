use std::time::Duration;

use anyhow::{Context as _, Result};
use gtl_web_contracts::test_ids;
use thirtyfour::{
    By, WebDriver, WebElement, prelude::ElementQueryable as _, stringmatch::StringMatch,
};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_refreshes_toggles_commit_and_restores_a_saved_live_diff() -> Result<()> {
    support::run_test("viewer-live-lifecycle", |session| {
        Box::pin(async move {
            let fixture = support::fixture::ViewerFixture::create(session.data_root())?;
            fixture.forward_live_view()?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v1")
                .await
                .context("show the forwarded live diff")?;

            fixture.commit_alpha_v2()?;
            support::selectors::by_test_id(session.driver(), test_ids::LIVE_VIEW_REFRESH)
                .await?
                .click()
                .await
                .context("refresh the live diff")?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2")
                .await
                .context("show the refreshed live diff")?;

            let commit_card =
                wait_for_commit_card_selection(session.driver(), "live view v2", false).await?;
            commit_card
                .click()
                .await
                .context("select the live view v2 commit")?;
            let commit_card =
                wait_for_commit_card_selection(session.driver(), "live view v2", true).await?;
            commit_card
                .click()
                .await
                .context("toggle off the live view v2 commit")?;
            wait_for_commit_card_selection(session.driver(), "live view v2", false).await?;

            session
                .restart_server()
                .await
                .context("restart gtl-server while the viewer remains open")?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2")
                .await
                .context("reconnect the open viewer to the replacement server")?;

            session
                .restart()
                .await
                .context("restart saved live viewer")?;
            support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2")
                .await
                .context("restore the refreshed live diff")
        })
    })
    .await
}

async fn wait_for_commit_card_selection(
    driver: &WebDriver,
    subject: &str,
    selected: bool,
) -> Result<WebElement> {
    let aria_pressed = if selected { "true" } else { "false" };
    driver
        .query(By::Css("[aria-label='Commits'] button[aria-pressed]"))
        .ignore_errors(true)
        .with_text(StringMatch::new(subject).partial())
        .with_attribute("aria-pressed", aria_pressed)
        .and_enabled()
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await
        .with_context(|| format!("find {subject} commit card aria-pressed={aria_pressed}"))
}
