use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, Key, WebDriver, prelude::ElementQueryable as _};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_controls_sidebars_across_tabs_and_restarts() -> Result<()> {
    support::run_test("viewer-desktop-shell", |session| {
        Box::pin(async move {
            let first = OneShotFixture::create_named(session.data_root(), "shell-alpha")?;
            let second = OneShotFixture::create_named(session.data_root(), "shell-beta")?;
            first.forward()?;
            support::wait_for_active_diff(session.driver(), "shell-alpha", "alpha-one-shot-marker")
                .await?;
            assert_sidebars(session.driver(), true, true).await?;
            sidebar_button(session.driver(), "Files")
                .await?
                .click()
                .await?;
            assert_sidebars(session.driver(), false, true).await?;
            session
                .driver()
                .query(By::Css("#files-sidebar-toggle:focus"))
                .wait(
                    wait::ASSERTION_TIMEOUT,
                    std::time::Duration::from_millis(100),
                )
                .first()
                .await?;
            shortcut(session.driver(), true).await?;
            assert_sidebars(session.driver(), false, false).await?;
            support::evidence::capture(session.driver(), "desktop-shell-hidden-sidebars", true)
                .await?;
            second.forward()?;
            support::wait_for_active_diff(session.driver(), "shell-beta", "alpha-one-shot-marker")
                .await?;
            assert_sidebars(session.driver(), false, false).await?;
            session.restart().await?;
            first.forward()?;
            support::wait_for_active_diff(session.driver(), "shell-alpha", "alpha-one-shot-marker")
                .await?;
            assert_sidebars(session.driver(), false, false).await?;
            shortcut(session.driver(), false).await?;
            assert_sidebars(session.driver(), true, false).await?;
            sidebar_button(session.driver(), "Commits")
                .await?
                .click()
                .await?;
            assert_sidebars(session.driver(), true, true).await?;
            verify_window_controls(session.driver()).await?;
            verify_window_drag_and_resize(session.driver()).await?;
            verify_narrow_shortcuts(session.driver()).await?;
            session
                .driver()
                .find(By::Css("button[aria-label='Close window']"))
                .await?
                .click()
                .await?;
            first.forward()?;
            support::wait_for_active_diff(session.driver(), "shell-alpha", "alpha-one-shot-marker")
                .await?;
            assert_sidebars(session.driver(), true, true).await?;
            support::evidence::capture(session.driver(), "desktop-shell-visible-sidebars", true)
                .await?;
            session.stop_server().await?;
            session
                .driver()
                .query(By::Css(".viewer-connection-notice"))
                .and_displayed()
                .wait(
                    wait::ASSERTION_TIMEOUT,
                    std::time::Duration::from_millis(100),
                )
                .first()
                .await?;
            verify_window_controls(session.driver()).await?;
            support::evidence::capture(session.driver(), "desktop-shell-disconnected", true)
                .await?;
            Ok(())
        })
    })
    .await
}

async fn sidebar_button(driver: &WebDriver, name: &str) -> Result<thirtyfour::WebElement> {
    Ok(driver
        .find(By::Css(format!(
            "button[aria-label='Toggle {name} sidebar']"
        )))
        .await?)
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
            && commits.is_displayed().await? == commits_visible
            && sidebar_button(driver, "Files").await?.is_enabled().await?
            && sidebar_button(driver, "Commits")
                .await?
                .is_enabled()
                .await?)
            .then_some(()))
    })
    .await
}

async fn shortcut(driver: &WebDriver, commits: bool) -> Result<()> {
    let chain = driver.action_chain().key_down(Key::Control);
    let chain = if commits {
        chain.key_down(Key::Alt)
    } else {
        chain
    };
    let chain = chain.send_keys("b");
    let chain = if commits {
        chain.key_up(Key::Alt)
    } else {
        chain
    };
    chain.key_up(Key::Control).perform().await?;
    Ok(())
}

async fn verify_window_controls(driver: &WebDriver) -> Result<()> {
    let maximize = driver
        .find(By::Css("button[aria-label='Maximize window']"))
        .await?;
    ensure!(maximize.is_displayed().await?, "maximize is unavailable");
    maximize.click().await?;
    wait::until("restore window action", wait::ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(By::Css("button[aria-label='Restore window']"))
            .await?
            .into_iter()
            .next())
    })
    .await?
    .click()
    .await?;
    wait::until(
        "maximize window action",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .find_all(By::Css("button[aria-label='Maximize window']"))
                .await?
                .into_iter()
                .next())
        },
    )
    .await?;
    ensure!(
        driver
            .find(By::Css("button[aria-label='Close window']"))
            .await?
            .is_enabled()
            .await?,
        "close is unavailable"
    );
    Ok(())
}

async fn verify_narrow_shortcuts(driver: &WebDriver) -> Result<()> {
    driver.set_window_rect(0, 0, 390, 800).await?;
    shortcut(driver, false).await?;
    wait::until("Files popover", wait::ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(By::Css("#mobile-files-panel[open]"))
            .await?
            .into_iter()
            .next())
    })
    .await?;
    shortcut(driver, false).await?;
    shortcut(driver, true).await?;
    wait::until("Commits popover", wait::ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(By::Css("#mobile-commits-panel[open]"))
            .await?
            .into_iter()
            .next())
    })
    .await?;
    shortcut(driver, true).await?;
    driver.set_window_rect(0, 0, 1280, 900).await?;
    assert_sidebars(driver, true, true).await
}

async fn verify_window_drag_and_resize(driver: &WebDriver) -> Result<()> {
    driver.set_window_rect(50, 50, 1000, 650).await?;
    let before = driver.get_window_rect().await?;
    let drag_region = driver.find(By::Css(".viewer-window-drag-region")).await?;
    let region = drag_region.rect().await?;
    native_drag(
        region.x + region.width / 2.0,
        region.y + region.height / 2.0,
    )?;
    wait::until(
        "native window dragging",
        wait::ASSERTION_TIMEOUT,
        || async {
            let after = driver.get_window_rect().await?;
            Ok((after.x != before.x || after.y != before.y).then_some(()))
        },
    )
    .await?;
    for (edge, horizontal, vertical) in [
        ("north", 1, 0),
        ("north-east", 2, 0),
        ("east", 2, 1),
        ("south-east", 2, 2),
        ("south", 1, 2),
        ("south-west", 0, 2),
        ("west", 0, 1),
        ("north-west", 0, 0),
    ] {
        driver.set_window_rect(150, 150, 1000, 650).await?;
        let before = driver.get_window_rect().await?;
        let (width, height) = native_window_size()?;
        native_drag(
            1.0 + f64::from(horizontal) * f64::from(width - 3) / 2.0,
            1.0 + f64::from(vertical) * f64::from(height - 3) / 2.0,
        )?;
        wait::until(
            &format!("native {edge} resizing"),
            wait::ASSERTION_TIMEOUT,
            || async {
                let after = driver.get_window_rect().await?;
                Ok(((after.width != before.width) == (horizontal != 1)
                    && (after.height != before.height) == (vertical != 1))
                    .then_some(()))
            },
        )
        .await?;
    }
    let region = drag_region.rect().await?;
    native_pointer(
        region.x + region.width / 2.0,
        region.y + region.height / 2.0,
        &[
            "mousedown",
            "1",
            "sleep",
            "0.08",
            "mouseup",
            "1",
            "sleep",
            "0.12",
            "mousedown",
            "1",
            "sleep",
            "0.08",
            "mouseup",
            "1",
        ],
    )?;
    driver
        .query(By::Css("button[aria-label='Restore window']"))
        .wait(
            wait::ASSERTION_TIMEOUT,
            std::time::Duration::from_millis(100),
        )
        .first()
        .await?
        .click()
        .await?;
    Ok(())
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

fn native_drag(x: f64, y: f64) -> Result<()> {
    native_pointer(
        x,
        y,
        &[
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
        ],
    )
}

fn native_pointer(x: f64, y: f64, actions: &[&str]) -> Result<()> {
    let result = std::process::Command::new("xdotool")
        .args([
            "getactivewindow",
            "mousemove",
            "--window",
            "%1",
            &format!("{x:.0}"),
            &format!("{y:.0}"),
        ])
        .args(actions)
        .output()?;
    ensure!(
        result.status.success(),
        "native pointer input failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}
