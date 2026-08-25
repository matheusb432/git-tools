use anyhow::{Context as _, Result, ensure};
use gtl_web_contracts::test_ids;
use serde::Deserialize;
use thirtyfour::{By, WebDriver, prelude::ElementQueryable as _};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_reopens_a_closed_snapshot_from_history() -> Result<()> {
    support::run_test("viewer-one-shot-lifecycle", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create(session.data_root())?;
            fixture.forward()?;
            support::wait_for_active_diff(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
            )
            .await?;
            let copied = support::copy_selected_diff_line(
                session.driver(),
                "work.txt",
                "alpha-one-shot-marker",
            )
            .await?;
            ensure!(
                copied == "// * work.txt, lines: 2\nalpha-one-shot-marker",
                "the desktop viewer copied an unexpected source payload: {copied:?}"
            );
            let copy_toast = support::selectors::by_test_id(session.driver(), test_ids::TOAST)
                .await
                .context("show copied-line feedback in the global viewport")?;
            ensure!(
                copy_toast.text().await?.starts_with("Copied with context"),
                "copied-line toast has unexpected text"
            );
            support::selectors::by_test_id(session.driver(), test_ids::TOAST_DISMISS)
                .await?
                .click()
                .await
                .context("dismiss copied-line feedback")?;
            wait::until(
                "dismissed copied-line feedback",
                wait::ASSERTION_TIMEOUT,
                || async {
                    Ok(session
                        .driver()
                        .find_all(By::Css(test_ids::TOAST.selector()))
                        .await?
                        .is_empty()
                        .then_some(()))
                },
            )
            .await?;
            assert_path_copy_popover(session.driver()).await?;

            support::selectors::by_test_id(session.driver(), test_ids::VIEWER_TAB_CLOSE)
                .await?
                .click()
                .await
                .context("close the snapshot")?;
            wait_for_empty_workspace(session.driver()).await?;
            ensure!(
                session
                    .driver()
                    .find_all(By::Css(test_ids::TOAST.selector()))
                    .await?
                    .is_empty(),
                "closing a snapshot enqueued a toast"
            );

            support::selectors::by_test_id(session.driver(), test_ids::VIEWER_MENU_TRIGGER)
                .await?
                .click()
                .await
                .context("open the viewer menu")?;
            support::selectors::by_test_id(session.driver(), test_ids::VIEWER_HISTORY_OPEN)
                .await?
                .click()
                .await
                .context("open diff history")?;
            support::selectors::by_test_id(session.driver(), test_ids::HISTORY_ENTRY_OPEN)
                .await?
                .click()
                .await
                .context("reopen the snapshot from history")?;

            support::wait_for_active_diff(
                session.driver(),
                "one-shot-alpha",
                "alpha-one-shot-marker",
            )
            .await
        })
    })
    .await
}

#[derive(Deserialize)]
struct PopoverGeometry {
    vertical_gap: f64,
    end_alignment: f64,
}

async fn assert_path_copy_popover(driver: &WebDriver) -> Result<()> {
    let file_selector = "[data-gtl-diff-file][data-path='work.txt']";
    let trigger = driver
        .find(By::Css(format!(
            "{file_selector} button[aria-label='Copy file path']"
        )))
        .await
        .context("find the desktop path popover trigger")?;
    trigger
        .click()
        .await
        .context("open the desktop path popover")?;
    let popover = driver
        .query(By::Css(format!(
            "{file_selector} [popover][aria-label='Copy file path']"
        )))
        .ignore_errors(true)
        .and_displayed()
        .wait(
            wait::ASSERTION_TIMEOUT,
            std::time::Duration::from_millis(100),
        )
        .first()
        .await
        .context("show the desktop path popover")?;
    let options = popover
        .text()
        .await
        .context("read the desktop path options")?;
    ensure!(
        options.contains("Relative path") && options.contains("Absolute path"),
        "desktop path popover is missing an option: {options:?}"
    );
    ensure!(
        popover
            .find_all(By::Css("[data-gtl-copy='code']"))
            .await
            .context("inspect desktop code-copy actions")?
            .is_empty(),
        "desktop path popover retained the removed code action"
    );
    let geometry: PopoverGeometry = driver
        .execute(
            r#"
                const file = document.querySelector(
                    "[data-gtl-diff-file][data-path='work.txt']",
                );
                const trigger = file.querySelector(
                    "button[aria-label='Copy file path']",
                );
                const popover = file.querySelector(
                    "[popover][aria-label='Copy file path']",
                );
                const triggerBox = trigger.getBoundingClientRect();
                const popoverBox = popover.getBoundingClientRect();
                return {
                    vertical_gap: popoverBox.top - triggerBox.bottom,
                    end_alignment: Math.abs(popoverBox.right - triggerBox.right),
                };
            "#,
            Vec::new(),
        )
        .await
        .context("measure the desktop path popover")?
        .convert()
        .context("decode the desktop path popover geometry")?;
    ensure!(
        (-0.5..=8.0).contains(&geometry.vertical_gap),
        "desktop path popover is not anchored below its trigger: gap {}",
        geometry.vertical_gap
    );
    ensure!(
        geometry.end_alignment <= 1.0,
        "desktop path popover is not end-aligned with its trigger: delta {}",
        geometry.end_alignment
    );

    trigger
        .click()
        .await
        .context("close the desktop path popover")?;
    wait::until(
        "closed desktop path popover",
        wait::ASSERTION_TIMEOUT,
        || async { Ok((!popover.is_displayed().await?).then_some(())) },
    )
    .await?;
    let file = driver
        .find(By::Css(format!("{file_selector}[open]")))
        .await
        .context("keep the desktop diff file expanded after using its path menu")?;
    ensure!(file.is_displayed().await?, "desktop diff file is hidden");
    Ok(())
}

async fn wait_for_empty_workspace(driver: &thirtyfour::WebDriver) -> Result<()> {
    wait::until("empty diff workspace", wait::ASSERTION_TIMEOUT, || async {
        if !driver.find_all(By::Css("[role='tab']")).await?.is_empty() {
            return Ok(None);
        }
        let main = driver.find(By::Css("main")).await?;
        Ok(main.text().await?.contains("No diff is open").then_some(()))
    })
    .await
}
