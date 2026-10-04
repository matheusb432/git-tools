use anyhow::{Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use crate::support::{self, fixture::ViewerFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_keeps_review_progress_when_refreshing_and_reopening_a_diff() -> Result<()> {
    support::run_test("diff-review-progress", |session| {
        Box::pin(async move {
            let driver = session.driver();
            let fixture = ViewerFixture::create(session.data_root())?;
            fixture.forward()?;
            support::wait_for_active_diff(driver, "live-view", "alpha-v1").await?;
            support::click(driver, review_action("work.txt", false)).await?;
            support::visible(driver, review_action("work.txt", true)).await?;
            toggle_review_shortcut(driver).await?;
            support::visible(driver, review_action("work.txt", false)).await?;
            toggle_review_shortcut(driver).await?;
            support::visible(driver, review_action("work.txt", true)).await?;

            support::click(
                driver,
                By::Css("[data-path='work.txt'] button[aria-label='Copy file path']"),
            )
            .await?;
            toggle_review_shortcut(driver).await?;
            driver
                .action_chain()
                .send_keys(Key::Escape)
                .perform()
                .await?;
            fixture.commit_extra()?;
            refresh(driver).await?;
            support::wait_for_active_diff(driver, "live-view", "additional-live-marker").await?;
            support::visible(driver, review_action("work.txt", true)).await?;

            // The focused second file takes precedence over the first visible file.
            support::click(driver, By::Css("[data-path='work.txt'] > summary")).await?;
            toggle_review_shortcut(driver).await?;
            support::visible(driver, review_action("work.txt", false)).await?;
            support::visible(driver, review_action("extra.txt", false)).await?;
            toggle_review_shortcut(driver).await?;
            support::visible(driver, review_action("work.txt", true)).await?;
            support::evidence::capture(driver, "diff-review-marked", true).await?;

            driver
                .action_chain()
                .send_keys(Key::Escape)
                .perform()
                .await?;
            support::click(driver, By::Css("button[aria-label='Filter files']")).await?;
            support::click(driver, By::Css("input[id$='-unreviewed']")).await?;
            driver
                .action_chain()
                .send_keys(Key::Escape)
                .perform()
                .await?;
            support::visible(
                driver,
                By::Css("button[data-file-target][title='extra.txt']"),
            )
            .await?;
            ensure!(
                driver
                    .find_all(By::Css("button[data-file-target][title='work.txt']"))
                    .await?
                    .is_empty(),
                "the unreviewed filter retained a reviewed file"
            );
            support::click(driver, review_action("extra.txt", false)).await?;
            support::visible(
                driver,
                By::XPath("//*[@role='status']//*[normalize-space(.)='All files reviewed']"),
            )
            .await?;
            support::evidence::capture(driver, "diff-review-complete", true).await?;
            support::click(
                driver,
                By::XPath("//button[normalize-space(.)='Show reviewed files']"),
            )
            .await?;
            support::visible(driver, review_action("work.txt", true)).await?;
            support::visible(driver, review_action("extra.txt", true)).await?;
            review_on_narrow_screen(driver).await?;

            session.restart_server().await?;
            session.restart().await?;
            let driver = session.driver();
            support::click(
                driver,
                By::XPath("//*[@role='tab' and contains(., 'live-view')]"),
            )
            .await?;
            support::wait_for_active_diff(driver, "live-view", "alpha-v1").await?;
            support::visible(driver, review_action("work.txt", true)).await?;
            support::visible(driver, review_action("extra.txt", true)).await?;
            fixture.commit_alpha_v2()?;
            refresh(driver).await?;
            support::wait_for_active_diff(driver, "live-view", "alpha-v2").await?;
            support::visible(driver, review_action("work.txt", false)).await?;
            Ok(())
        })
    })
    .await
}

fn review_action(path: &str, reviewed: bool) -> By {
    By::Css(format!(
        "[data-path='{path}'] button[aria-label='Reviewed'][aria-pressed='{reviewed}']"
    ))
}

async fn review_on_narrow_screen(driver: &WebDriver) -> Result<()> {
    driver.set_window_rect(20, 20, 600, 700).await?;
    wait::until(
        "narrow review touch target",
        wait::ASSERTION_TIMEOUT,
        || async {
            let action = driver.find(review_action("work.txt", true)).await?;
            let rect = action.rect().await?;
            Ok((rect.width >= 44.0 && rect.height >= 44.0).then_some(()))
        },
    )
    .await?;
    support::click(driver, review_action("work.txt", true)).await?;
    support::visible(driver, review_action("work.txt", false)).await?;
    support::click(driver, review_action("work.txt", false)).await?;
    support::visible(driver, review_action("work.txt", true)).await?;
    support::evidence::capture(driver, "diff-review-narrow", true).await?;
    driver.set_window_rect(20, 20, 1200, 800).await?;
    Ok(())
}

async fn toggle_review_shortcut(driver: &WebDriver) -> Result<()> {
    driver
        .action_chain()
        .key_down(Key::Alt)
        .send_keys("r")
        .key_up(Key::Alt)
        .perform()
        .await?;
    Ok(())
}

async fn refresh(driver: &WebDriver) -> Result<()> {
    let tab = support::visible(driver, By::Css("[role='tab'][aria-selected='true']")).await?;
    support::context_click_element(driver, &tab).await?;
    support::click(
        driver,
        By::XPath("//*[@role='menu' and @aria-label='Tab actions']//*[@role='menuitem' and normalize-space(.)='Refresh']"),
    )
    .await?;
    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await?;
    Ok(())
}
