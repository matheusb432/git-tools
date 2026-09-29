use anyhow::{Result, ensure};
use gtl_web_contracts::test_ids;
use thirtyfour::{By, Key, WebDriver};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_reviews_a_cli_snapshot_and_reopens_it_from_history() -> Result<()> {
    support::run_test("cli-review", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create_with_long_lines(session.data_root())?;
            fixture.forward()?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "long-lines", "alpha-one-shot-marker").await?;
            copy_commit_id_without_selecting(driver).await?;

            support::click(driver, By::Css("a[aria-label='Projects']")).await?;
            fixture.forward()?;
            support::wait_for_active_diff(driver, "long-lines", "alpha-one-shot-marker").await?;
            ensure!(
                driver.find_all(By::Css("[role='tab']")).await?.len() == 1,
                "opening the same diff again created another tab"
            );

            find_in_all_files(driver, "alpha-one-shot-marker").await?;
            let copied =
                support::copy_selected_diff_line(driver, "work.txt", "alpha-one-shot-marker")
                    .await?;
            ensure!(
                copied == "// * work.txt, lines: 2\nalpha-one-shot-marker",
                "copy omitted the file context: {copied:?}"
            );
            copy_truncated_line(driver).await?;

            rename_snapshot(driver, "Auth review").await?;
            let tab =
                support::visible(driver, By::Css("[role='tab'][aria-selected='true']")).await?;
            support::context_click_element(driver, &tab).await?;
            let details = support::visible(
                driver,
                By::Css("[role='menu'][aria-label='Tab actions']:popover-open"),
            )
            .await?
            .text()
            .await?;
            ensure!(
                details.contains("long-lines") && details.contains("feature"),
                "renaming hid the snapshot's source: {details}"
            );
            driver
                .action_chain()
                .send_keys(Key::Escape)
                .perform()
                .await?;
            support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_CLOSE)
                .await?
                .click()
                .await?;
            support::visible(driver, By::Css("#projects-heading")).await?;
            support::selectors::by_test_id(driver, test_ids::VIEWER_HISTORY_OPEN)
                .await?
                .click()
                .await?;
            support::selectors::by_test_id(driver, test_ids::HISTORY_ENTRY_OPEN)
                .await?
                .click()
                .await?;
            support::visible(
                driver,
                By::XPath("//*[@role='tab' and contains(., 'Auth review')]"),
            )
            .await?;
            support::wait_for_active_diff(driver, "Auth review", "alpha-one-shot-marker").await
        })
    })
    .await
}

async fn copy_commit_id_without_selecting(driver: &WebDriver) -> Result<()> {
    let copy_button = support::visible(
        driver,
        By::Css("aside[aria-label='Commits'] button[title='Copy commit ID']"),
    )
    .await?;
    copy_button.click().await?;
    wait::until("commit ID copied", wait::ASSERTION_TIMEOUT, || async {
        Ok(copy_button.text().await?.contains("Copied").then_some(()))
    })
    .await?;
    ensure!(
        driver
            .find_all(By::Css("aside[aria-label='Commits'] [aria-pressed='true']"))
            .await?
            .is_empty(),
        "copying the commit ID selected the commit"
    );
    Ok(())
}

async fn find_in_all_files(driver: &WebDriver, text: &str) -> Result<()> {
    driver
        .action_chain()
        .key_down(Key::Control)
        .send_keys("f")
        .key_up(Key::Control)
        .perform()
        .await?;
    support::visible(driver, By::Css("#viewer-diff-find-input"))
        .await?
        .send_keys(text)
        .await?;
    wait::until("one search match", wait::ASSERTION_TIMEOUT, || async {
        let region = driver
            .find(By::Css(
                "[role='search'][aria-label='Find code in all files']",
            ))
            .await?;
        Ok(region.text().await?.contains("1 match").then_some(()))
    })
    .await?;
    support::click(driver, By::Css("button[aria-label='Close search']")).await
}

async fn copy_truncated_line(driver: &WebDriver) -> Result<()> {
    support::click(driver, By::Css("button[data-file-target][title='a.css']")).await?;
    support::wait_for_active_diff(driver, "long-lines", "94218 characters omitted").await?;
    let copied = support::copy_selected_diff_line(driver, "a.css", "xxx").await?;
    ensure!(
        copied.ends_with(&"x".repeat(94_718)) && !copied.contains("characters omitted"),
        "copying a truncated line did not return its complete source"
    );
    Ok(())
}

async fn rename_snapshot(driver: &WebDriver, name: &str) -> Result<()> {
    let tab = support::visible(driver, By::Css("[role='tab'][aria-selected='true']")).await?;
    support::context_click_element(driver, &tab).await?;
    wait::until(
        "Rename snapshot action",
        wait::ASSERTION_TIMEOUT,
        || async {
            let menu = driver
                .find(By::Css(
                    "[role='menu'][aria-label='Tab actions']:popover-open",
                ))
                .await?;
            menu.find(By::XPath(".//button[normalize-space(.)='Rename snapshot']"))
                .await?
                .click()
                .await?;
            Ok(Some(()))
        },
    )
    .await?;
    let editor = support::visible(driver, By::Css("input[aria-label='Snapshot name']")).await?;
    editor.send_keys(name).await?;
    editor.send_keys(Key::Enter).await?;
    support::visible(
        driver,
        By::XPath(format!("//*[@role='tab' and contains(., '{name}')]")),
    )
    .await?;
    Ok(())
}
