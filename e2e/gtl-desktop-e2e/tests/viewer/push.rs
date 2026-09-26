use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, WebDriver};

use crate::support::{self, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_pushes_exactly_the_reviewed_commit() -> Result<()> {
    support::run_test("push", |session| {
        Box::pin(async move {
            let directory = tempfile::Builder::new()
                .prefix(".gtl-push-")
                .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
            let fixture = support::fixture::PushFixture::create(directory.path())?;
            fixture.forward_snapshot(session.data_root())?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "push-review", "push-latest-marker").await?;

            support::click(driver, By::Id("viewer-push-trigger")).await?;
            wait_for_review(driver, &fixture.latest).await?;

            support::click(
                driver,
                By::XPath("//dialog[@id='viewer-push-confirmation']//button[normalize-space()='Cancel']"),
            ).await?;
            support::click(driver, By::Id("commits-sidebar-toggle")).await?;
            support::visible(driver, By::Css("#commits-sidebar-toggle[aria-pressed='false']")).await?;
            let tab = support::visible(driver, By::Css("[role='tab'][aria-selected='true']")).await?;
            support::context_click_element(driver, &tab).await?;
            support::click(
                driver,
                By::XPath("//*[@role='menu' and @aria-label='Tab actions']//button[normalize-space()='Push']"),
            ).await?;
            wait_for_review(driver, &fixture.latest).await?;

            fixture.add_newer()?;
            support::click(
                driver,
                By::XPath(
                    "//dialog[@id='viewer-push-confirmation']//button[starts-with(normalize-space(), 'Push ')]",
                ),
            )
            .await?;
            support::dismiss_toast(driver, "Push completed.").await?;
            ensure!(
                fixture.remote_head()? == fixture.latest,
                "the push did not stop at the reviewed commit"
            );
            ensure!(
                fixture.repository.join("untracked.txt").exists(),
                "the push changed untracked work"
            );
            support::click(driver, By::Id("commits-sidebar-toggle")).await?;
            support::visible(driver, By::Css("#viewer-push-trigger:disabled")).await?;
            Ok(())
        })
    })
    .await
}

async fn wait_for_review(driver: &WebDriver, commit: &str) -> Result<()> {
    wait::until(
        "review of the selected SHA and its destination",
        wait::ASSERTION_TIMEOUT,
        || async {
            let review = driver
                .find(By::Css("#viewer-push-confirmation[open]"))
                .await?
                .text()
                .await?;
            Ok((review.contains(commit)
                && review.contains("Branch")
                && review.contains("Remote")
                && review.contains("origin"))
            .then_some(()))
        },
    )
    .await
}
