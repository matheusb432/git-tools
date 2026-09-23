use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, Key, WebDriver, WebElement, prelude::ElementQueryable as _};

use crate::{
    settings_editing::{open_settings, select_value},
    support::{self, fixture::OneShotFixture, wait},
};

#[tokio::test(flavor = "multi_thread")]
async fn interface_scale_and_reduced_motion_survive_restart() -> Result<()> {
    support::run_test("accessibility", |session| {
        Box::pin(async move {
            session.driver().set_window_rect(0, 0, 3840, 2160).await?;
            open_settings(session.driver()).await?;
            let width = viewport_width(session.driver()).await?;
            ensure!(width >= 3800.0, "4K window was not available: {width}");
            select_value(session.driver(), "settings-ui-scale", "200").await?;
            select_value(session.driver(), "settings-reduce-motion", "true").await?;
            ensure!(
                (viewport_width(session.driver()).await? - width).abs() < 2.0,
                "interface scale changed before saving"
            );
            save_settings(session.driver()).await?;
            assert_viewport_width(session.driver(), width / 2.0).await?;
            assert_reduced_motion(session.driver()).await?;
            support::evidence::capture(session.driver(), "accessibility-4k-settings-200", true)
                .await?;

            let fixture = OneShotFixture::create_named(session.data_root(), "large-display")?;
            fixture.forward()?;
            support::wait_for_active_diff(
                session.driver(),
                "large-display",
                "alpha-one-shot-marker",
            )
            .await?;
            ensure!(
                session
                    .driver()
                    .find(By::Css("aside[aria-label='Changed files']"))
                    .await?
                    .is_displayed()
                    .await?
            );
            support::evidence::capture(session.driver(), "accessibility-4k-diff-200", true).await?;

            session.restart().await?;
            session.driver().set_window_rect(0, 0, 3840, 2160).await?;
            assert_viewport_width(session.driver(), width / 2.0).await?;
            assert_reduced_motion(session.driver()).await?;
            session.driver().set_window_rect(0, 0, 1280, 900).await?;
            assert_viewport_width(session.driver(), 640.0).await?;
            let menu = support::selectors::by_test_id(
                session.driver(),
                gtl_web_contracts::test_ids::VIEWER_MENU_TRIGGER,
            )
            .await?;
            click_scaled(&menu, 2.0).await?;
            session
                .driver()
                .query(By::Css("button[aria-label='User settings']"))
                .and_displayed()
                .first()
                .await?
                .send_keys(Key::Enter)
                .await?;
            session
                .driver()
                .query(By::Id("settings-ui-scale"))
                .and_displayed()
                .first()
                .await?;
            session.driver().set_window_rect(0, 0, 3840, 2160).await?;
            ensure!(
                session
                    .driver()
                    .find(By::Id("settings-ui-scale"))
                    .await?
                    .value()
                    .await?
                    .as_deref()
                    == Some("200")
            );

            select_value(session.driver(), "settings-ui-scale", "300").await?;
            save_settings(session.driver()).await?;
            assert_viewport_width(session.driver(), width / 3.0).await?;
            session.driver().set_window_rect(0, 0, 1280, 900).await?;
            ensure!(
                session
                    .driver()
                    .find(By::Id("settings-ui-scale"))
                    .await?
                    .is_displayed()
                    .await?
            );
            select_value(session.driver(), "settings-ui-scale", "100").await?;
            select_value(session.driver(), "settings-reduce-motion", "false").await?;
            save_settings(session.driver()).await?;
            assert_viewport_width(session.driver(), 1280.0).await?;
            session
                .driver()
                .query(By::Css("html[data-reduce-motion='false']"))
                .first()
                .await?;
            Ok(())
        })
    })
    .await
}

async fn viewport_width(driver: &WebDriver) -> Result<f64> {
    Ok(driver
        .execute("return window.innerWidth", Vec::new())
        .await?
        .convert()?)
}

async fn assert_viewport_width(driver: &WebDriver, expected: f64) -> Result<()> {
    wait::until(
        "interface zoom applied",
        wait::ASSERTION_TIMEOUT,
        || async { Ok(((viewport_width(driver).await? - expected).abs() < 3.0).then_some(())) },
    )
    .await
}

async fn assert_reduced_motion(driver: &WebDriver) -> Result<()> {
    driver
        .query(By::Css("html[data-reduce-motion='true']"))
        .first()
        .await?;
    let menu = driver
        .find(By::Css("button[data-testid='viewer-menu-trigger']"))
        .await?;
    ensure!(menu.css_value("transition-property").await? == "none");
    let moving: Vec<String> = driver.execute(
        "return [...document.querySelectorAll('*')].filter(e => getComputedStyle(e).animationName !== 'none').map(e => e.tagName)",
        Vec::new(),
    ).await?.convert()?;
    ensure!(moving.is_empty(), "animations remained enabled: {moving:?}");
    Ok(())
}

async fn save_settings(driver: &WebDriver) -> Result<()> {
    driver
        .find(By::XPath("//button[normalize-space()='Save settings']"))
        .await?
        .send_keys(Key::Enter)
        .await?;
    Ok(())
}

async fn click_scaled(element: &WebElement, scale: f64) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // WebKitWebDriver's element click uses unscaled widget coordinates at native zoom.
        let rect = element.rect().await?;
        let output = std::process::Command::new("timeout")
            .args([
                "5s",
                "xdotool",
                "search",
                "--onlyvisible",
                "--name",
                "^git-tools diff viewer$",
                "mousemove",
                "--window",
                "%1",
                &format!("{:.0}", (rect.x + rect.width / 2.0) * scale),
                &format!("{:.0}", (rect.y + rect.height / 2.0) * scale),
                "click",
                "1",
            ])
            .output()
            .context("click an enlarged control through native pointer input")?;
        ensure!(
            output.status.success(),
            "native click failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = scale;
        element.click().await?;
    }
    Ok(())
}
