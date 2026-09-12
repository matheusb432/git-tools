use anyhow::{Result, ensure};
use thirtyfour::{By, WebDriver};

use crate::support::{self, fixture::OneShotFixture, wait};

const PANELS: [&str; 2] = [".diff-files-scroll-panel", ".diff-commits-scroll-panel"];

#[tokio::test(flavor = "multi_thread")]
async fn side_panel_scroll_positions_belong_to_each_tab() -> Result<()> {
    support::run_test("viewer-panel-scroll", |session| {
        Box::pin(run_panel_scroll(session))
    })
    .await
}

async fn run_panel_scroll(session: &mut support::session::TestSession) -> Result<()> {
    let first = OneShotFixture::create_with_panel_history(session.data_root(), "scroll-first")?;
    let second = OneShotFixture::create_with_panel_history(session.data_root(), "scroll-second")?;
    let short = OneShotFixture::create_named(session.data_root(), "scroll-short")?;
    let driver = session.driver();
    driver.set_window_rect(0, 0, 1400, 800).await?;

    first.forward()?;
    support::wait_for_active_diff(driver, "scroll-first", "panel scroll fixture").await?;
    scroll_panels(driver, [360.0, 720.0]).await?;

    second.forward()?;
    support::wait_for_active_diff(driver, "scroll-second", "panel scroll fixture").await?;
    assert_panel_positions(driver, [0.0, 0.0]).await?;
    scroll_panels(driver, [160.0, 320.0]).await?;

    short.forward()?;
    support::wait_for_active_diff(driver, "scroll-short", "alpha-one-shot-marker").await?;
    assert_panel_positions(driver, [0.0, 0.0]).await?;
    ensure!(
        driver
            .find(By::Css(".diff-files-scroll-panel [data-file-target]"))
            .await?
            .is_displayed()
            .await?,
        "the short tab's file panel disappeared after switching tabs"
    );

    for (name, positions) in [
        ("scroll-first", [360.0, 720.0]),
        ("scroll-second", [160.0, 320.0]),
        ("scroll-first", [360.0, 720.0]),
    ] {
        driver
            .find(By::Css(format!("button[role='tab'][title*='{name}']")))
            .await?
            .click()
            .await?;
        support::wait_for_active_diff(driver, name, "panel scroll fixture").await?;
        assert_panel_positions(driver, positions).await?;
    }

    driver
        .find(By::Css("a[aria-label='Projects']"))
        .await?
        .click()
        .await?;
    driver
        .find(By::Css("button[role='tab'][title*='scroll-first']"))
        .await?
        .click()
        .await?;
    support::wait_for_active_diff(driver, "scroll-first", "panel scroll fixture").await?;
    assert_panel_positions(driver, [360.0, 720.0]).await?;
    support::evidence::capture(driver, "panel-scroll-restored", true).await?;
    Ok(())
}

async fn scroll_panels(driver: &WebDriver, positions: [f64; 2]) -> Result<()> {
    for (selector, position) in PANELS.into_iter().zip(positions) {
        let panel = driver.find(By::Css(selector)).await?;
        driver
            .execute(
                "arguments[0].scrollTop = arguments[1];",
                vec![panel.to_json()?, position.into()],
            )
            .await?;
    }
    assert_panel_positions(driver, positions).await
}

async fn assert_panel_positions(driver: &WebDriver, positions: [f64; 2]) -> Result<()> {
    wait::until(
        "per-tab side panel scroll positions",
        wait::ASSERTION_TIMEOUT,
        || async {
            for (selector, expected) in PANELS.into_iter().zip(positions) {
                let panel = driver.find(By::Css(selector)).await?;
                let actual = driver
                    .execute("return arguments[0].scrollTop;", vec![panel.to_json()?])
                    .await?
                    .convert::<f64>()?;
                if (actual - expected).abs() > 1.0 {
                    return Ok(None);
                }
            }
            Ok(Some(()))
        },
    )
    .await
}
