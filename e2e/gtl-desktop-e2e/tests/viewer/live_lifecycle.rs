use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use gtl_web_contracts::test_ids;
use thirtyfour::{By, WebDriver, WebElement, prelude::ElementQueryable as _};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn live_diff_updates_automatically_and_restores_after_reconnect() -> Result<()> {
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
    ensure!(
        session
            .driver()
            .find_all(By::Css("[aria-label='Refresh diff']"))
            .await?
            .is_empty(),
        "manual refresh remained visible"
    );
    support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2")
        .await
        .context("show the refreshed live diff")?;

    let commit_card =
        wait_for_commit_card_selection(session.driver(), "live view v2", false).await?;
    commit_card
        .click()
        .await
        .context("select the live view v2 commit")?;
    wait_for_commit_card_selection(session.driver(), "live view v2", true).await?;
    fixture.commit_extra()?;
    wait_for_commit_card_selection(session.driver(), "live view extra", false).await?;
    let commit_card =
        wait_for_commit_card_selection(session.driver(), "live view v2", true).await?;
    commit_card
        .click()
        .await
        .context("toggle off the live view v2 commit")?;
    wait_for_commit_card_selection(session.driver(), "live view v2", false).await?;

    exercise_warning_recovery(session, &fixture).await?;

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
    session
        .driver()
        .query(By::Id("projects-heading"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    session
        .driver()
        .query(By::Css("[role='tab'][title*='live-view']"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2")
        .await
        .context("restore the refreshed live diff")?;

    close_live_view(session).await
}

async fn exercise_warning_recovery(
    session: &support::session::TestSession,
    fixture: &support::fixture::ViewerFixture,
) -> Result<()> {
    fixture.make_git_unavailable()?;
    let warning =
        support::selectors::by_test_id(session.driver(), test_ids::LIVE_VIEW_WARNING).await?;
    session
        .driver()
        .action_chain()
        .move_to_element_center(&warning)
        .perform()
        .await?;
    session
        .driver()
        .query(By::Css(
            "[role='tooltip'][aria-label='Live diff update warnings']",
        ))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    support::wait_for_active_diff(session.driver(), "live-view", "alpha-v2").await?;
    support::evidence::capture(session.driver(), "viewer-live-warning", true).await?;
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    let warning =
        support::selectors::by_test_id(session.driver(), test_ids::LIVE_VIEW_WARNING).await?;
    session
        .driver()
        .action_chain()
        .move_to_element_center(&warning)
        .perform()
        .await?;
    session
        .driver()
        .query(By::Css(
            "[role='tooltip'][aria-label='Live diff update warnings']",
        ))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    support::evidence::capture(session.driver(), "viewer-live-warning-narrow", true).await?;
    session.driver().set_window_rect(0, 0, 1200, 800).await?;
    let warning =
        support::selectors::by_test_id(session.driver(), test_ids::LIVE_VIEW_WARNING).await?;
    session
        .driver()
        .action_chain()
        .move_to_element_center(&warning)
        .perform()
        .await?;
    fixture.restore_git()?;
    support::wait::until(
        "live warnings clear after successful recovery",
        Duration::from_secs(75),
        || async {
            Ok(session
                .driver()
                .find_all(By::Css(test_ids::LIVE_VIEW_WARNING.selector()))
                .await?
                .is_empty()
                .then_some(()))
        },
    )
    .await?;

    fixture.make_git_unavailable()?;
    let warning =
        support::selectors::by_test_id(session.driver(), test_ids::LIVE_VIEW_WARNING).await?;
    session
        .driver()
        .action_chain()
        .move_to(0, 0)
        .move_to_element_center(&warning)
        .perform()
        .await?;
    session
        .driver()
        .query(By::Css(
            "[role='tooltip'][aria-label='Live diff update warnings']",
        ))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    fixture.restore_git()?;

    Ok(())
}

async fn close_live_view(session: &mut support::session::TestSession) -> Result<()> {
    let driver = session.driver();
    ensure!(
        driver
            .find_all(By::Css(
                "[aria-label='Live view actions'], #delete-live-view-dialog"
            ))
            .await?
            .is_empty(),
        "manual live deletion remains available"
    );
    support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_CLOSE)
        .await?
        .click()
        .await?;
    driver
        .query(By::Id("projects-heading"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        driver
            .find_all(By::Css(test_ids::TOAST.selector()))
            .await?
            .is_empty(),
        "closing a live tab enqueued a toast"
    );
    support::evidence::capture(driver, "live-view-closed", true).await?;
    session.restart().await?;
    session
        .driver()
        .query(By::Id("projects-heading"))
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css("[role='tab']"))
            .await?
            .is_empty(),
        "closed live view was restored after restart"
    );
    Ok(())
}

async fn wait_for_commit_card_selection(
    driver: &WebDriver,
    subject: &str,
    selected: bool,
) -> Result<WebElement> {
    let aria_pressed = if selected { "true" } else { "false" };
    driver
        .query(By::Css(format!(
            "[aria-label='Commits'] button[aria-label*='{subject}'][aria-pressed='{aria_pressed}']"
        )))
        .ignore_errors(true)
        .and_enabled()
        .and_displayed()
        .wait(support::wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await
        .with_context(|| format!("find {subject} commit card aria-pressed={aria_pressed}"))
}
