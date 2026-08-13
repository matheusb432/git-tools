use anyhow::{Context as _, Result};
use gtl_web_contracts::test_ids;
use thirtyfour::By;

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_reopens_a_closed_snapshot_from_history() -> Result<()> {
    support::run_test("viewer-one-shot-lifecycle", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create(session.data_root())?;
            fixture.forward()?;
            support::wait_for_active_diff(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
            )
            .await?;

            support::selectors::by_test_id(session.driver(), test_ids::VIEWER_TAB_CLOSE)
                .await?
                .click()
                .await
                .context("close the snapshot")?;
            wait_for_empty_workspace(session.driver()).await?;

            support::selectors::by_test_id(session.driver(), test_ids::VIEWER_MENU_TRIGGER)
                .await?
                .click()
                .await
                .context("open the viewer menu")?;
            support::selectors::by_test_id(session.driver(), test_ids::VIEWER_HISTORY_OPEN)
                .await?
                .click()
                .await
                .context("open diff history")?;
            support::selectors::by_test_id(session.driver(), test_ids::HISTORY_ENTRY_OPEN)
                .await?
                .click()
                .await
                .context("reopen the snapshot from history")?;

            support::wait_for_active_diff(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
            )
            .await
        })
    })
    .await
}

async fn wait_for_empty_workspace(driver: &thirtyfour::WebDriver) -> Result<()> {
    wait::until("empty diff workspace", wait::ASSERTION_TIMEOUT, || async {
        if !driver.find_all(By::Css("[role='tab']")).await?.is_empty() {
            return Ok(None);
        }
        let main = driver.find(By::Css("main")).await?;
        Ok(main.text().await?.contains("No diff is open").then_some(()))
    })
    .await
}
