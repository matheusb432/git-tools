use anyhow::{Context as _, Result, ensure};
use gtl_web_contracts::test_ids;
use serde::Deserialize;
use thirtyfour::{
    By, Key, WebDriver, WebElement, components::SelectElement, prelude::ElementQueryable as _,
};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_reopens_a_closed_snapshot_from_history() -> Result<()> {
    support::run_test("viewer-one-shot-lifecycle", |session| {
        Box::pin(run_one_shot_lifecycle(session))
    })
    .await
}

async fn run_one_shot_lifecycle(session: &mut support::session::TestSession) -> Result<()> {
    session.write_user_config(
        r#"
[keybindings]
search_files = "alt+p"
search_text_in_all_files = "alt+f"
"#,
    )?;
    session
        .restart()
        .await
        .context("restart the viewer with custom search shortcuts")?;
    let fixture = OneShotFixture::create(session.data_root())?;
    fixture.forward()?;
    support::wait_for_active_diff(session.driver(), "one-shot-alpha", "alpha-one-shot-marker")
        .await?;
    assert_cli_reopen_navigates_from_settings_and_projects(session.driver(), &fixture).await?;
    assert_menu_covers_active_scrollbar(session.driver()).await?;
    assert_menu_keyboard_navigation(session.driver()).await?;
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
    wait_for_projects_home(session.driver()).await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css(test_ids::TOAST.selector()))
            .await?
            .is_empty(),
        "closing a snapshot enqueued a toast"
    );

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

async fn assert_cli_reopen_navigates_from_settings_and_projects(
    driver: &WebDriver,
    fixture: &OneShotFixture,
) -> Result<()> {
    let diff_url = driver.current_url().await?;
    ensure!(
        diff_url.path().starts_with("/diffs/"),
        "the diff has no tab route: {diff_url}"
    );
    for (label, route) in [("User settings", "/settings"), ("Projects", "/projects")] {
        if route == "/settings" {
            support::selectors::by_test_id(driver, test_ids::VIEWER_MENU_TRIGGER)
                .await?
                .click()
                .await?;
        }
        driver
            .find(By::Css(format!(
                "button[aria-label='{label}'], a[aria-label='{label}']"
            )))
            .await?
            .click()
            .await?;
        wait::until(
            "viewer secondary route",
            wait::ASSERTION_TIMEOUT,
            || async { Ok((driver.current_url().await?.path() == route).then_some(())) },
        )
        .await?;
        if route == "/settings" {
            assert_settings_update_preserves_route(driver).await?;
        }
        fixture.forward()?;
        support::wait_for_active_diff(driver, "one-shot-alpha", "alpha-one-shot-marker").await?;
        ensure!(
            driver.current_url().await? == diff_url,
            "reopening the same diff changed its tab route"
        );
        ensure!(
            driver.find_all(By::Css("[role='tab']")).await?.len() == 1,
            "reopening the diff created another tab"
        );
        wait::until(
            "CLI diff keyboard focus",
            wait::ASSERTION_TIMEOUT,
            || async {
                Ok((driver.active_element().await?.attr("id").await?.as_deref()
                    == Some("workspace-heading"))
                .then_some(()))
            },
        )
        .await?;
        driver.back().await?;
        wait::until(
            "Back restores the preceding page",
            wait::ASSERTION_TIMEOUT,
            || async { Ok((driver.current_url().await?.path() == route).then_some(())) },
        )
        .await?;
        driver
            .find(By::Css("[role='tab']"))
            .await?
            .send_keys(Key::Right)
            .await?;
        support::wait_for_active_diff(driver, "one-shot-alpha", "alpha-one-shot-marker").await?;
        ensure!(
            driver.current_url().await? == diff_url,
            "keyboard tab navigation did not restore its route"
        );
        ensure!(
            driver
                .active_element()
                .await?
                .attr("role")
                .await?
                .as_deref()
                == Some("tab"),
            "keyboard tab navigation moved focus out of the tab rail"
        );
    }
    Ok(())
}

async fn assert_settings_update_preserves_route(driver: &WebDriver) -> Result<()> {
    let theme = driver.query(By::Id("settings-theme")).first().await?;
    SelectElement::new(&theme)
        .await?
        .select_by_value("dark")
        .await?;
    driver
        .find(By::Css("button[type='submit']"))
        .await?
        .click()
        .await?;
    wait::until("saved settings", wait::ASSERTION_TIMEOUT, || async {
        let toasts = driver.find_all(By::Css(test_ids::TOAST.selector())).await?;
        for toast in toasts {
            if toast.text().await?.contains("Settings saved") {
                return Ok(Some(()));
            }
        }
        Ok(None)
    })
    .await?;
    ensure!(
        driver.current_url().await?.path() == "/settings",
        "saving settings navigated to a diff"
    );
    support::selectors::by_test_id(driver, test_ids::TOAST_DISMISS)
        .await?
        .click()
        .await?;
    Ok(())
}

async fn assert_menu_keyboard_navigation(driver: &WebDriver) -> Result<()> {
    let trigger = support::selectors::by_test_id(driver, test_ids::VIEWER_MENU_TRIGGER).await?;
    trigger.send_keys(Key::Up).await?;
    for (key, label) in [
        (Key::Home, "User settings"),
        (Key::Down, "User settings"),
        (Key::Down, "User settings"),
        (Key::End, "User settings"),
    ] {
        driver.active_element().await?.send_keys(key).await?;
        ensure!(
            driver
                .active_element()
                .await?
                .attr("aria-label")
                .await?
                .as_deref()
                == Some(label),
            "menu keyboard navigation did not focus {label}"
        );
    }
    driver.active_element().await?.send_keys("s").await?;
    ensure!(
        driver
            .active_element()
            .await?
            .attr("aria-label")
            .await?
            .as_deref()
            == Some("User settings"),
        "menu letter navigation did not focus Settings"
    );
    driver
        .active_element()
        .await?
        .send_keys(Key::Escape)
        .await?;
    ensure!(
        driver.active_element().await? == trigger,
        "Escape did not restore menu trigger focus"
    );
    trigger.send_keys(Key::Enter).await?;
    ensure!(
        driver
            .active_element()
            .await?
            .attr("aria-label")
            .await?
            .as_deref()
            == Some("User settings"),
        "Enter did not focus the first menu action"
    );
    driver.active_element().await?.send_keys(Key::Tab).await?;
    ensure!(
        driver
            .find_all(By::Css("[role='menu']:popover-open"))
            .await?
            .is_empty(),
        "Tab did not dismiss the menu"
    );
    ensure!(
        driver
            .active_element()
            .await?
            .attr("role")
            .await?
            .as_deref()
            != Some("menuitem"),
        "Tab kept focus inside the menu"
    );
    Ok(())
}

async fn assert_menu_covers_active_scrollbar(driver: &WebDriver) -> Result<()> {
    let trigger = support::selectors::by_test_id(driver, test_ids::VIEWER_MENU_TRIGGER).await?;
    let document = driver.find(By::Css("[data-gtl-diff-document]")).await?;
    let document_rect = document.rect().await?;
    for _ in 0..2 {
        trigger.click().await?;
        let menu = driver
            .find(By::Css("[popover][aria-label='Viewer menu']"))
            .await?;
        let menu_rect = menu.rect().await?;
        let scrollbar_x = document_rect.x + document_rect.width - 7.0;
        ensure!(
            scrollbar_x > menu_rect.x
                && scrollbar_x < menu_rect.x + menu_rect.width
                && document_rect.y < menu_rect.y + menu_rect.height,
            "the viewer menu must overlap the diff scrollbar in this fixture"
        );
        driver
            .action_chain()
            .move_to_element_center(&trigger)
            .perform()
            .await?;
        let scrollbar = document
            .find(By::Css(
                ":scope > .scrollbars [data-scroll-axis='vertical']",
            ))
            .await?;
        ensure!(
            scrollbar.is_displayed().await?,
            "opening the menu hid the background scrollbar"
        );
        let unobstructed = menu.screenshot_as_png().await?;
        driver
            .action_chain()
            .move_to(
                format!("{scrollbar_x:.0}").parse()?,
                format!("{:.0}", menu_rect.y + menu_rect.height + 24.0).parse()?,
            )
            .perform()
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        ensure!(
            menu.screenshot_as_png().await? == unobstructed,
            "the active diff scrollbar paints through the viewer menu"
        );
        support::evidence::capture(driver, "viewer-menu-over-scrollbar", true).await?;
        trigger.send_keys(Key::Escape).await?;
    }
    Ok(())
}

async fn assert_commit_details_hover_popover(driver: &WebDriver) -> Result<()> {
    let commits = support::selectors::by_test_id(driver, test_ids::COMMITS_PANEL).await?;
    let card = commits
        .find(By::Css("[data-gtl-hover-popover-target]"))
        .await
        .context("find the desktop commit hover target")?;
    ensure!(
        card.find_all(By::Css("button")).await?.len() == 1,
        "the commit card retained a separate details trigger"
    );
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
    let animation_style = read_commit_popover_animation_style(driver, &popover).await?;
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
    popover: &WebElement,
) -> Result<CommitPopoverAnimationStyle> {
    let surface = popover
        .find(By::Css(test_ids::HOVER_POPOVER_CONTENT.selector()))
        .await
        .context("find the desktop commit popover content")?;
    Ok(driver
        .execute(
            r#"
                const surface = arguments[0];

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
            vec![surface.to_json().context("encode commit popover content")?],
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
    ensure!(
        file_panel.find_all(By::Css("kbd")).await?.is_empty(),
        "changed-file filter displays a shortcut hint"
    );
    driver
        .action_chain()
        .key_down(Key::Alt)
        .send_keys("p")
        .key_up(Key::Alt)
        .perform()
        .await
        .context("focus the changed-file filter with configured Alt+P")?;
    let file_filter = driver
        .query(By::Css("input[placeholder='Filter files by path']"))
        .and_displayed()
        .wait(
            wait::ASSERTION_TIMEOUT,
            std::time::Duration::from_millis(100),
        )
        .first()
        .await
        .context("open changed-file filter popup")?;
    wait::until(
        "configured file-search shortcut focuses the filter",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(
                (driver.active_element().await?.element_id() == file_filter.element_id())
                    .then_some(()),
            )
        },
    )
    .await?;
    file_filter
        .send_keys("missing-file")
        .await
        .context("filter changed files through the server")?;
    wait::until(
        "server-filtered empty file list",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(driver
                .find(By::Css(
                    "[role='search'][aria-label='Filter files by path']",
                ))
                .await?
                .text()
                .await?
                .contains("No files match")
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
            Ok(driver
                .find_all(By::Css("[role='option'][aria-label='work.txt']"))
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
    driver
        .find(By::Css("input[placeholder='Filter files by path']"))
        .await
        .context("find focused changed-file filter")?
        .send_keys(Key::Alt + "f")
        .await
        .context("open diff search with configured Alt+F from an input")?;
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
        find_region.find_all(By::Css("kbd")).await?.is_empty(),
        "diff search displays a shortcut hint"
    );
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

async fn wait_for_projects_home(driver: &thirtyfour::WebDriver) -> Result<()> {
    wait::until(
        "Projects home after closing the last tab",
        wait::ASSERTION_TIMEOUT,
        || async {
            if !driver.find_all(By::Css("[role='tab']")).await?.is_empty() {
                return Ok(None);
            }
            let main = driver
                .query(By::Css("main"))
                .and_displayed()
                .first()
                .await?;
            Ok(main.text().await?.contains("Projects").then_some(()))
        },
    )
    .await
}
