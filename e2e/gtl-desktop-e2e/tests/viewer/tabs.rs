use anyhow::{Context as _, Result};
use gtl_web_contracts::test_ids;
use thirtyfour::{By, Key, WebDriver};

use crate::support::{self, fixture::OneShotFixture, wait};

const DESKTOP_WIDTH: u32 = 1400;
const NARROW_WIDTH: u32 = 480;
const WINDOW_HEIGHT: u32 = 800;
const REPOSITORIES: [&str; 3] = ["tabs-first", "tabs-second", "tabs-third"];

#[tokio::test(flavor = "multi_thread")]
async fn user_arranges_pins_and_closes_many_diff_tabs() -> Result<()> {
    support::run_test("tabs", |session| {
        Box::pin(async move {
            let driver = session.driver();
            driver
                .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
                .await?;
            open_snapshots(session.data_root())?;
            support::wait_for_active_diff(driver, "tabs-third", "alpha-one-shot-marker").await?;

            support::click(driver, tab("tabs-first")).await?;
            support::wait_for_active_diff(driver, "tabs-first", "alpha-one-shot-marker").await?;
            driver.back().await?;
            support::wait_for_active_diff(driver, "tabs-third", "alpha-one-shot-marker").await?;

            drag_tab(driver, "tabs-first", "tabs-third").await?;
            wait_for_tab_order(driver, &["tabs-second", "tabs-third", "tabs-first"]).await?;

            let inactive = support::visible(driver, tab("tabs-second")).await?;
            support::context_click_element(driver, &inactive).await?;
            support::click(
                driver,
                By::Css("[role='menu'][aria-label='Tab actions'] [aria-keyshortcuts='Alt+p']"),
            )
            .await?;
            support::visible(driver, By::Css("button[aria-label^='Unpin ']")).await?;
            press(driver, Key::Alt, "o").await?;
            wait_for_tab_order(driver, &["tabs-second", "tabs-first"]).await?;
            support::wait_for_active_diff(driver, "tabs-first", "alpha-one-shot-marker").await?;

            use_overflow_menu(driver).await?;
            Ok(())
        })
    })
    .await
}

fn open_snapshots(data_root: &std::path::Path) -> Result<()> {
    for repository in REPOSITORIES {
        OneShotFixture::create_named(data_root, repository)?.forward()?;
    }
    Ok(())
}

fn tab(repository: &str) -> By {
    By::Css(format!("[role='tab'][title*='{repository}']"))
}

async fn drag_tab(driver: &WebDriver, source: &str, target: &str) -> Result<()> {
    let source = support::visible(driver, tab(source)).await?.rect().await?;
    let target = support::visible(driver, tab(target)).await?.rect().await?;
    let (source_x, source_y) = center(
        source.x + source.width / 2.0,
        source.y + source.height / 2.0,
    );
    let (target_x, _) = center(target.x + target.width / 2.0, target.y);
    driver
        .action_chain()
        .move_to(source_x, source_y)
        .click_and_hold()
        .move_to(source_x + 12, source_y + 40)
        .perform()
        .await?;
    support::visible(driver, By::Css("[data-viewer-tab-floating='true']")).await?;
    driver
        .action_chain()
        .move_to(target_x + 8, source_y + 40)
        .perform()
        .await?;
    driver.action_chain().release().perform().await?;
    Ok(())
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "window coordinates fit in pointer action integers"
)]
fn center(x: f64, y: f64) -> (i64, i64) {
    (x.round() as i64, y.round() as i64)
}

async fn wait_for_tab_order(driver: &WebDriver, expected: &[&str]) -> Result<()> {
    wait::until("dropped tab order", wait::ASSERTION_TIMEOUT, || async {
        let mut titles = Vec::new();
        for tab in driver
            .find_all(By::Css(
                "[data-viewer-tab-rail-content='true'] [role='tab']",
            ))
            .await?
        {
            titles.push(tab.attr("title").await?.unwrap_or_default());
        }
        let ordered = titles.len() == expected.len()
            && titles
                .iter()
                .zip(expected)
                .all(|(title, repository)| title.contains(repository));
        Ok(ordered.then_some(()))
    })
    .await
}

/// Narrows the window until the rail collapses, then switches and closes tabs from its menu.
async fn use_overflow_menu(driver: &WebDriver) -> Result<()> {
    driver
        .set_window_rect(20, 20, NARROW_WIDTH, WINDOW_HEIGHT)
        .await?;
    support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_OVERFLOW_TRIGGER)
        .await?
        .click()
        .await?;
    let menu = support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_OVERFLOW_MENU).await?;
    menu.find(By::XPath(".//li/button[contains(., 'tabs-second')]"))
        .await
        .context("find tabs-second in the overflow menu")?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "tabs-second", "alpha-one-shot-marker").await?;
    support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_OVERFLOW_TRIGGER)
        .await?
        .click()
        .await?;
    support::click(
        driver,
        By::Css(format!(
            "{} button[aria-label^='Close '][aria-label*='tabs-first']",
            test_ids::VIEWER_TAB_OVERFLOW_MENU.selector()
        )),
    )
    .await?;
    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await?;
    driver
        .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
        .await?;
    wait_for_tab_order(driver, &["tabs-second"]).await
}

async fn press(driver: &WebDriver, modifier: Key, key: &str) -> Result<()> {
    driver
        .action_chain()
        .key_down(modifier.clone())
        .send_keys(key)
        .key_up(modifier)
        .perform()
        .await?;
    Ok(())
}
