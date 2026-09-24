use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use crate::{
    settings::{open_settings, select_value},
    support::{self, fixture::OneShotFixture, wait},
};

#[tokio::test(flavor = "multi_thread")]
async fn desktop_layout_window_and_scale_survive_restart() -> Result<()> {
    support::run_test("desktop-shell", |session| {
        Box::pin(async move {
            let first = OneShotFixture::create_named(session.data_root(), "shell-alpha")?;
            let second = OneShotFixture::create_named(session.data_root(), "shell-beta")?;
            first.forward()?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "shell-alpha", "alpha-one-shot-marker").await?;

            support::click(driver, By::Css("button[aria-label='Toggle Files sidebar']")).await?;
            assert_sidebars(driver, false, true).await?;
            driver
                .action_chain()
                .key_down(Key::Control)
                .key_down(Key::Alt)
                .send_keys("b")
                .key_up(Key::Alt)
                .key_up(Key::Control)
                .perform()
                .await?;
            assert_sidebars(driver, false, false).await?;
            second.forward()?;
            support::wait_for_active_diff(driver, "shell-beta", "alpha-one-shot-marker").await?;
            assert_sidebars(driver, false, false).await?;

            use_window_controls(driver).await?;
            support::click(driver, By::Css("button[aria-label='Close window']")).await?;
            first.forward()?;
            support::wait_for_active_diff(driver, "shell-alpha", "alpha-one-shot-marker").await?;

            driver.set_window_rect(0, 0, 1280, 900).await?;
            open_settings(driver).await?;
            select_value(driver, "settings-ui-scale", "200").await?;
            support::click(driver, By::Css("button[type='submit']")).await?;
            wait_for_viewport_width(driver, 640.0).await?;

            session.restart().await?;
            let driver = session.driver();
            driver.set_window_rect(0, 0, 1280, 900).await?;
            wait_for_viewport_width(driver, 640.0).await?;
            first.forward()?;
            support::wait_for_active_diff(driver, "shell-alpha", "alpha-one-shot-marker").await?;
            assert_sidebars(driver, false, false).await
        })
    })
    .await
}

async fn assert_sidebars(
    driver: &WebDriver,
    files_visible: bool,
    commits_visible: bool,
) -> Result<()> {
    wait::until("sidebar visibility", wait::ASSERTION_TIMEOUT, || async {
        let files = driver
            .find(By::Css("aside[aria-label='Changed files']"))
            .await?;
        let commits = driver.find(By::Css("aside[aria-label='Commits']")).await?;
        Ok((files.is_displayed().await? == files_visible
            && commits.is_displayed().await? == commits_visible)
            .then_some(()))
    })
    .await
}

async fn use_window_controls(driver: &WebDriver) -> Result<()> {
    support::click(driver, By::Css("button[aria-label='Maximize window']")).await?;
    support::click(driver, By::Css("button[aria-label='Restore window']")).await?;
    support::visible(driver, By::Css("button[aria-label='Maximize window']")).await?;

    driver.set_window_rect(50, 50, 1000, 650).await?;
    let before = driver.get_window_rect().await?;
    let region = support::visible(driver, By::Css(".viewer-window-drag-region"))
        .await?
        .rect()
        .await?;
    native_drag(
        region.x + region.width / 2.0,
        region.y + region.height / 2.0,
    )?;
    wait::until("window moved", wait::ASSERTION_TIMEOUT, || async {
        let after = driver.get_window_rect().await?;
        Ok((after.x != before.x || after.y != before.y).then_some(()))
    })
    .await?;

    driver.set_window_rect(150, 150, 1000, 650).await?;
    let before = driver.get_window_rect().await?;
    let (width, height) = native_window_size()?;
    native_drag(f64::from(width - 2), f64::from(height - 2))?;
    wait::until("window resized", wait::ASSERTION_TIMEOUT, || async {
        let after = driver.get_window_rect().await?;
        Ok((after.width != before.width && after.height != before.height).then_some(()))
    })
    .await?;
    Ok(())
}

async fn wait_for_viewport_width(driver: &WebDriver, expected: f64) -> Result<()> {
    wait::until(
        "interface scale applied",
        wait::ASSERTION_TIMEOUT,
        || async {
            let width: f64 = driver
                .execute("return window.innerWidth", Vec::new())
                .await?
                .convert()?;
            Ok(((width - expected).abs() < 3.0).then_some(()))
        },
    )
    .await
}

fn native_window_size() -> Result<(u32, u32)> {
    let result = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowgeometry", "--shell"])
        .output()?;
    ensure!(result.status.success(), "native window measurement failed");
    let output = std::str::from_utf8(&result.stdout)?;
    let dimension = |prefix| -> Result<u32> {
        Ok(output
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .context("native window dimension is absent")?
            .parse()?)
    };
    Ok((dimension("WIDTH=")?, dimension("HEIGHT=")?))
}

/// Drags with native pointer input, because the Linux frame owns moving and resizing.
fn native_drag(x: f64, y: f64) -> Result<()> {
    let result = std::process::Command::new("xdotool")
        .args([
            "getactivewindow",
            "mousemove",
            "--window",
            "%1",
            &format!("{x:.0}"),
            &format!("{y:.0}"),
            "mousedown",
            "1",
            "sleep",
            "0.2",
            "mousemove_relative",
            "--",
            "40",
            "30",
            "sleep",
            "0.2",
            "mouseup",
            "1",
        ])
        .output()?;
    ensure!(
        result.status.success(),
        "native pointer input failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}
