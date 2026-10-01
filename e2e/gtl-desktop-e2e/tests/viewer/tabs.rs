use anyhow::Result;
use thirtyfour::{By, Key, WebDriver};

use crate::support::{self, fixture::OneShotFixture, wait};

const DESKTOP_WIDTH: u32 = 1400;
const NARROW_WIDTH: u32 = 480;
const WINDOW_HEIGHT: u32 = 800;
const REPOSITORIES: [&str; 3] = [
    "tabs-first-with-a-title-that-fades-beside-the-close-button",
    "tabs-second",
    "tabs-third",
];

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

            press(driver, Key::Control, Key::Tab).await?;
            support::wait_for_active_diff(driver, "tabs-second", "alpha-one-shot-marker").await?;
            press(driver, Key::Control, Key::Tab).await?;
            support::wait_for_active_diff(driver, "tabs-first", "alpha-one-shot-marker").await?;
            press(driver, Key::Control, Key::Tab).await?;
            support::wait_for_active_diff(driver, "tabs-third", "alpha-one-shot-marker").await?;
            driver
                .action_chain()
                .key_down(Key::Control)
                .key_down(Key::Shift)
                .send_keys(Key::Tab)
                .key_up(Key::Shift)
                .key_up(Key::Control)
                .perform()
                .await?;
            support::wait_for_active_diff(driver, "tabs-first", "alpha-one-shot-marker").await?;
            press(driver, Key::Control, Key::Tab).await?;
            support::wait_for_active_diff(driver, "tabs-third", "alpha-one-shot-marker").await?;

            support::click(driver, tab("tabs-first")).await?;
            support::wait_for_active_diff(driver, "tabs-first", "alpha-one-shot-marker").await?;
            let close = support::visible(
                driver,
                By::Css(".viewer-tab-close[aria-label*='tabs-first']"),
            )
            .await?;
            driver
                .action_chain()
                .move_to_element_center(&close)
                .perform()
                .await?;
            support::evidence::capture(driver, "tab-close-hover-and-title-fade", true).await?;
            driver.back().await?;
            support::wait_for_active_diff(driver, "tabs-third", "alpha-one-shot-marker").await?;

            drag_tab(driver, "tabs-first", "tabs-third").await?;
            wait_for_tab_order(driver, &["tabs-third", "tabs-first", "tabs-second"]).await?;

            let inactive = support::visible(driver, tab("tabs-second")).await?;
            support::context_click_element(driver, &inactive).await?;
            support::click(
                driver,
                By::Css("[role='menu'][aria-label='Tab actions'] [aria-keyshortcuts='Alt+p' i]"),
            )
            .await?;
            support::visible(driver, By::Css("button[aria-label^='Unpin ']")).await?;
            press(driver, Key::Alt, "o").await?;
            wait_for_tab_order(driver, &["tabs-second", "tabs-first"]).await?;
            support::wait_for_active_diff(driver, "tabs-first", "alpha-one-shot-marker").await?;

            use_scrolling_rail(driver).await?;
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
    By::XPath(format!("//*[@role='tab' and contains(., '{repository}')]"))
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
        let mut labels = Vec::new();
        for tab in driver
            .find_all(By::Css(
                "[data-viewer-tab-rail-content='true'] [role='tab']",
            ))
            .await?
        {
            labels.push(tab.prop("textContent").await?.unwrap_or_default());
        }
        let ordered = labels.len() == expected.len()
            && labels
                .iter()
                .zip(expected)
                .all(|(label, repository)| label.contains(repository));
        Ok(ordered.then_some(()))
    })
    .await
}

async fn use_scrolling_rail(driver: &WebDriver) -> Result<()> {
    driver
        .set_window_rect(20, 20, NARROW_WIDTH, WINDOW_HEIGHT)
        .await?;
    let active = support::visible(driver, By::Css("[role='tab'][aria-selected='true']")).await?;
    active.send_keys(Key::Home).await?;
    support::wait_for_active_diff(driver, "tabs-second", "alpha-one-shot-marker").await?;
    let active = support::visible(driver, By::Css("[role='tab'][aria-selected='true']")).await?;
    active.send_keys(Key::End).await?;
    support::wait_for_active_diff(driver, "tabs-first", "alpha-one-shot-marker").await?;
    support::evidence::capture(driver, "scrolling-tabs-narrow", true).await?;
    support::click(driver, By::Id("review-close-trigger")).await?;
    support::wait_for_active_diff(driver, "tabs-second", "alpha-one-shot-marker").await?;
    wait::until(
        "closed diff focuses the next tab",
        wait::ASSERTION_TIMEOUT,
        || async {
            let focused = driver.active_element().await?;
            let selected_tab = focused.attr("role").await?.as_deref() == Some("tab")
                && focused.attr("aria-selected").await?.as_deref() == Some("true");
            Ok(selected_tab.then_some(()))
        },
    )
    .await?;
    driver
        .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
        .await?;
    wait_for_tab_order(driver, &["tabs-second"]).await
}

async fn press(
    driver: &WebDriver,
    modifier: Key,
    key: impl Into<thirtyfour::TypingData>,
) -> Result<()> {
    driver
        .action_chain()
        .key_down(modifier.clone())
        .send_keys(key)
        .key_up(modifier)
        .perform()
        .await?;
    Ok(())
}
