use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use gtl_web_contracts::test_ids;
use thirtyfour::{
    By, WebDriver, WebElement, prelude::ElementQueryable as _, stringmatch::StringMatch,
};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_refreshes_toggles_commit_and_restores_a_saved_live_diff() -> Result<()> {
    support::run_test("viewer-live-lifecycle", |session| {
        Box::pin(run_live_lifecycle(session))
    })
    .await
}

async fn run_live_lifecycle(session: &mut support::session::TestSession) -> Result<()> {
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
        .context("restore the refreshed live diff")?;

    delete_live_view_and_assert_toast(session.driver()).await
}

async fn delete_live_view_and_assert_toast(driver: &WebDriver) -> Result<()> {
    enqueue_copy_toast(driver).await?;
    confirm_live_view_deletion(driver).await?;
    assert_live_deletion_queue(driver).await
}

async fn enqueue_copy_toast(driver: &WebDriver) -> Result<()> {
    let copied = support::copy_selected_diff_line(driver, "work.txt", "alpha-v2").await?;
    ensure!(
        copied.contains("alpha-v2"),
        "live-view source copy omitted its selected line"
    );
    let copy_toast = support::selectors::by_test_id(driver, test_ids::TOAST)
        .await
        .context("show copied-line feedback before live deletion")?;
    ensure!(
        copy_toast.text().await?.starts_with("Copied with context"),
        "copied-line toast has unexpected text"
    );
    Ok(())
}

async fn confirm_live_view_deletion(driver: &WebDriver) -> Result<()> {
    driver
        .find(By::Id("mobile-display-trigger"))
        .await
        .context("find the compact display controls")?
        .click()
        .await
        .context("open the compact display controls")?;
    driver
        .find(By::Id("delete-live-view-mobile"))
        .await
        .context("find the live-view delete action")?
        .click()
        .await
        .context("open the live-view delete confirmation")?;
    let dialog = driver
        .find(By::Id("delete-live-view-dialog"))
        .await
        .context("find the live-view delete confirmation")?;
    dialog
        .query(By::Css("button"))
        .ignore_errors(true)
        .with_text(StringMatch::new("Delete live view"))
        .and_enabled()
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await
        .context("find the live-view delete confirmation action")?
        .click()
        .await
        .context("confirm the live-view deletion")?;
    Ok(())
}

async fn assert_live_deletion_queue(driver: &WebDriver) -> Result<()> {
    support::selectors::by_test_id(driver, test_ids::TOAST_LEDGER)
        .await
        .context("show the event ledger while live deletion waits")?;
    support::evidence::capture(driver, "viewer-toast-queued", true)
        .await
        .context("capture queued toast evidence")?;

    support::selectors::by_test_id(driver, test_ids::TOAST_DISMISS)
        .await?
        .click()
        .await
        .context("dismiss copied-line feedback")?;
    support::wait::until(
        "live-view deletion toast reaches the front of the queue",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            let toasts = driver.find_all(By::Css(test_ids::TOAST.selector())).await?;
            let Some(toast) = toasts.into_iter().next() else {
                return Ok(None);
            };
            Ok((toast.text().await? == "Live view deleted.").then_some(()))
        },
    )
    .await?;
    let toast = support::selectors::by_test_id(driver, test_ids::TOAST).await?;
    ensure!(
        toast.text().await? == "Live view deleted.",
        "live-view deletion toast has unexpected text"
    );
    ensure!(
        toast.attr("role").await?.as_deref() == Some("status"),
        "live-view deletion toast is not an accessible status"
    );
    ensure!(
        driver
            .find_all(By::Css(test_ids::TOAST_LEDGER.selector()))
            .await?
            .is_empty(),
        "event ledger remained visible with no waiting toast"
    );
    support::evidence::capture(driver, "viewer-toast-single", true)
        .await
        .context("capture single toast evidence")?;

    support::selectors::by_test_id(driver, test_ids::TOAST_DISMISS)
        .await?
        .click()
        .await
        .context("dismiss the live-view deletion toast")?;
    support::wait::until(
        "dismissed live-view deletion toast",
        support::wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .find_all(By::Css(test_ids::TOAST.selector()))
                .await?
                .is_empty()
                .then_some(()))
        },
    )
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
