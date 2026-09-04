use anyhow::{Context as _, Result, ensure};
use gtl_web_contracts::test_ids;
use serde::Deserialize;
use thirtyfour::{By, Key, WebDriver, prelude::ElementQueryable as _};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_reopens_a_closed_snapshot_from_history() -> Result<()> {
    support::run_test("viewer-one-shot-lifecycle", |session| {
        Box::pin(run_one_shot_lifecycle(session))
    })
    .await
}

async fn run_one_shot_lifecycle(session: &mut support::session::TestSession) -> Result<()> {
    let fixture = OneShotFixture::create(session.data_root())?;
    fixture.forward()?;
    support::wait_for_active_diff(session.driver(), "one-shot-alpha", "alpha-one-shot-marker")
        .await?;
    assert_server_owned_searches(session.driver()).await?;
    let copied =
        support::copy_selected_diff_line(session.driver(), "work.txt", "alpha-one-shot-marker")
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
    assert_commit_details_hover_popover(session.driver()).await?;
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

    support::wait_for_active_diff(session.driver(), "one-shot-alpha", "alpha-one-shot-marker").await
}

async fn assert_commit_details_hover_popover(driver: &WebDriver) -> Result<()> {
    let commits = support::selectors::by_test_id(driver, test_ids::COMMITS_PANEL).await?;
    ensure!(
        commits.find_all(By::Css("button")).await?.len() == 1,
        "the commit shelf retained a separate details trigger"
    );
    let card = commits
        .find(By::Css("[data-gtl-hover-popover-target]"))
        .await
        .context("find the desktop commit hover target")?;
    let popover = card
        .find(By::Css("[popover][role='tooltip']"))
        .await
        .context("find the desktop commit details popover")?;
    ensure!(
        !popover.is_displayed().await?,
        "desktop commit details are visible before hover"
    );

    driver
        .action_chain_with_delay(None, Some(std::time::Duration::ZERO))
        .move_to_element_center(&card)
        .perform()
        .await
        .context("hover the desktop commit card")?;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    ensure!(
        !popover.is_displayed().await?,
        "desktop commit details opened before the 350-millisecond hover delay"
    );
    wait::until(
        "visible desktop commit details after sustained hover",
        wait::ASSERTION_TIMEOUT,
        || async { Ok(popover.is_displayed().await?.then_some(())) },
    )
    .await?;
    let animation_style = read_commit_popover_animation_style(driver).await?;
    ensure!(
        animation_style.animation_name == "commit-popover-enter"
            && animation_style.animation_duration == "0.1s"
            && animation_style.animation_timing_function == "linear"
            && animation_style.overflow_x == "hidden"
            && animation_style.overflow_y == "auto"
            && animation_style.opacity_only,
        "desktop commit details use unexpected animation styles: {animation_style:?}"
    );
    let details = popover
        .text()
        .await
        .context("read desktop commit details")?;
    ensure!(
        details.contains("one-shot change")
            && details.contains("Date")
            && details.contains("Commit ID")
            && !details.contains("Committed"),
        "desktop commit details are incomplete: {details:?}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let card_rect = card
        .rect()
        .await
        .context("measure the desktop commit card")?;
    let popover_rect = popover
        .rect()
        .await
        .context("measure the desktop commit details")?;
    ensure!(
        (popover_rect.y - card_rect.y).abs() <= 1.0,
        "desktop commit details are detached from the card top"
    );
    ensure!(
        popover_rect.x + popover_rect.width <= card_rect.x - 4.0,
        "desktop commit details do not float beside the card: card={card_rect:?}, popover={popover_rect:?}"
    );
    support::evidence::capture(driver, "viewer-commit-details-hover", true).await?;

    let diff_document = driver
        .find(By::Css("[data-gtl-diff-document]"))
        .await
        .context("find the desktop diff document")?;
    driver
        .action_chain()
        .move_to_element_center(&diff_document)
        .perform()
        .await
        .context("leave the desktop commit card")?;
    wait::until(
        "hidden desktop commit details",
        wait::ASSERTION_TIMEOUT,
        || async { Ok((!popover.is_displayed().await?).then_some(())) },
    )
    .await
}

async fn read_commit_popover_animation_style(
    driver: &WebDriver,
) -> Result<CommitPopoverAnimationStyle> {
    Ok(driver
        .execute(
            r#"
                const surface = document.querySelector(
                    "[popover][role='tooltip'] > .animate-commit-popover-enter",
                );
                if (surface === null) {
                    return null;
                }

                const findKeyframes = (rules, name) => {
                    for (const rule of rules) {
                        if (rule instanceof CSSKeyframesRule && rule.name === name) {
                            return Array.from(rule.cssRules);
                        }
                        if ("cssRules" in rule) {
                            const nested = findKeyframes(rule.cssRules, name);
                            if (nested !== null) {
                                return nested;
                            }
                        }
                    }
                    return null;
                };
                const style = getComputedStyle(surface);
                let keyframes = null;
                for (const sheet of document.styleSheets) {
                    try {
                        keyframes = findKeyframes(sheet.cssRules, style.animationName);
                    } catch {
                        continue;
                    }
                    if (keyframes !== null) {
                        break;
                    }
                }

                return {
                    animationName: style.animationName,
                    animationDuration: style.animationDuration,
                    animationTimingFunction: style.animationTimingFunction,
                    overflowX: style.overflowX,
                    overflowY: style.overflowY,
                    opacityOnly: keyframes !== null && keyframes.length > 0 &&
                        keyframes.every((keyframe) =>
                            Array.from(keyframe.style).every((property) =>
                                property === "opacity"
                            )
                        ),
                };
            "#,
            Vec::new(),
        )
        .await?
        .convert()?)
}

async fn assert_server_owned_searches(driver: &WebDriver) -> Result<()> {
    assert_server_owned_file_search(driver).await?;
    assert_server_owned_diff_search(driver).await
}

async fn assert_server_owned_file_search(driver: &WebDriver) -> Result<()> {
    let file_panel = support::selectors::by_test_id(driver, test_ids::CHANGED_FILES_PANEL).await?;
    let file_filter = file_panel
        .find(By::Css("input[placeholder^='Filter paths']"))
        .await
        .context("find changed-file filter")?;
    file_filter
        .send_keys("missing-file")
        .await
        .context("filter changed files through the server")?;
    wait::until(
        "server-filtered empty file list",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(file_panel
                .text()
                .await?
                .contains("no files match this filter")
                .then_some(()))
        },
    )
    .await?;
    file_filter
        .send_keys(Key::Control + "a")
        .await
        .context("select the changed-file filter")?;
    file_filter
        .send_keys("work")
        .await
        .context("replace the changed-file filter")?;
    wait::until(
        "server-filtered changed file",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(file_panel
                .find_all(By::Css("button[title='work.txt']"))
                .await?
                .into_iter()
                .next()
                .map(|_| ()))
        },
    )
    .await?;

    Ok(())
}

async fn assert_server_owned_diff_search(driver: &WebDriver) -> Result<()> {
    let intercepted: bool = driver
        .execute(
            r"
                const target = document.querySelector('[data-gtl-diff-document]');
                const event = new KeyboardEvent('keydown', {
                    key: 'f',
                    ctrlKey: true,
                    bubbles: true,
                    cancelable: true,
                });
                target.dispatchEvent(event);
                return event.defaultPrevented;
            ",
            Vec::new(),
        )
        .await
        .context("open diff search with Ctrl+F")?
        .convert()
        .context("decode Ctrl+F interception")?;
    ensure!(intercepted, "the viewer did not suppress native Ctrl+F");
    let find_input = driver
        .query(By::Id("viewer-diff-find-input"))
        .ignore_errors(true)
        .and_displayed()
        .wait(
            wait::ASSERTION_TIMEOUT,
            std::time::Duration::from_millis(100),
        )
        .first()
        .await
        .context("show the diff search input")?;
    find_input
        .send_keys("alpha-one-shot-marker")
        .await
        .context("search rendered diff rows through the server")?;
    wait::until(
        "server-owned diff match",
        wait::ASSERTION_TIMEOUT,
        || async {
            let matches = driver.find_all(By::Css("[data-gtl-find-active]")).await?;
            let Some(found) = matches.into_iter().next() else {
                return Ok(None);
            };
            Ok(found
                .text()
                .await?
                .contains("alpha-one-shot-marker")
                .then_some(()))
        },
    )
    .await?;
    let find_region = driver
        .find(By::Css(
            "[role='search'][aria-label='Find code in all files']",
        ))
        .await
        .context("find diff search controls")?;
    ensure!(
        find_region.text().await?.contains("1 match"),
        "diff search did not report its server match count"
    );
    find_region
        .find(By::Css("button[aria-label='Close search']"))
        .await?
        .click()
        .await
        .context("close diff search")?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitPopoverAnimationStyle {
    animation_name: String,
    animation_duration: String,
    animation_timing_function: String,
    overflow_x: String,
    overflow_y: String,
    opacity_only: bool,
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

    copy_desktop_path_and_expect_popover_closed(&popover, "Copy relative path").await?;

    trigger
        .click()
        .await
        .context("reopen the desktop path popover")?;
    wait::until(
        "reopened desktop path popover",
        wait::ASSERTION_TIMEOUT,
        || async { Ok(popover.is_displayed().await?.then_some(())) },
    )
    .await?;
    copy_desktop_path_and_expect_popover_closed(&popover, "Copy absolute path").await?;

    let file = driver
        .find(By::Css(format!("{file_selector}[open]")))
        .await
        .context("keep the desktop diff file expanded after using its path menu")?;
    ensure!(file.is_displayed().await?, "desktop diff file is hidden");
    Ok(())
}

async fn copy_desktop_path_and_expect_popover_closed(
    popover: &thirtyfour::WebElement,
    aria_label: &str,
) -> Result<()> {
    popover
        .find(By::Css(format!("button[aria-label='{aria_label}']")))
        .await
        .with_context(|| format!("find the desktop {aria_label} action"))?
        .click()
        .await
        .with_context(|| format!("activate the desktop {aria_label} action"))?;
    wait::until(
        &format!("closed desktop path popover after {aria_label}"),
        wait::ASSERTION_TIMEOUT,
        || async { Ok((!popover.is_displayed().await?).then_some(())) },
    )
    .await
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
