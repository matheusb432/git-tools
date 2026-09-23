use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, ensure};
use gtl_web_contracts::test_ids;
use serde::Deserialize;
use thirtyfour::{
    By, Key, WebDriver, WebElement,
    prelude::{ElementQueryable as _, ElementWaitable as _},
};

use crate::support::{self, fixture::TabOverflowFixture, wait};

const DESKTOP_WIDTH: u32 = 1400;
const NARROW_WIDTH: u32 = 480;
const WINDOW_HEIGHT: u32 = 800;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabCloseVisual {
    background_color: String,
    center_delta_x: f64,
    center_delta_y: f64,
    has_title: bool,
    opacity: f64,
    stroke: String,
    transition_duration: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabSelectionMotion {
    active_transition_duration: String,
    inactive_transition_duration: String,
}

#[derive(Clone, Copy, Debug)]
enum TabDragAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabDragVisual {
    tabs: Vec<TabDragItemVisual>,
    floating: Vec<TabFloatingVisual>,
    reduced_motion: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabDragItemVisual {
    id: String,
    x: f64,
    y: f64,
    center_x: i32,
    center_y: i32,
    dragging: bool,
    transform_x: f64,
    transform_y: f64,
    transition_property: String,
    transition_duration: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabFloatingVisual {
    x: f64,
    y: f64,
    inert: bool,
    popover_manual: bool,
    popover_open: bool,
}

#[tokio::test(flavor = "multi_thread")]
async fn tab_rail_collapses_only_while_its_tabs_overflow() -> Result<()> {
    support::run_test("viewer-tab-overflow", |session| {
        Box::pin(run_tab_overflow(session))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn tab_actions_work_after_right_click_release_and_from_shortcuts() -> Result<()> {
    support::run_test("viewer-tab-actions", |session| {
        Box::pin(run_tab_actions(session))
    })
    .await
}

async fn run_tab_actions(session: &mut support::session::TestSession) -> Result<()> {
    let driver = session.driver();
    driver
        .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
        .await?;
    for name in [
        "tab-actions-first",
        "tab-actions-second",
        "tab-actions-third",
    ] {
        support::fixture::OneShotFixture::create_named(session.data_root(), name)?.forward()?;
    }
    support::wait_for_active_diff(driver, "tab-actions-third", "alpha-one-shot-marker").await?;
    let active_url = driver.current_url().await?;
    let inactive = driver
        .find(By::Css("[role='tab'][aria-selected='false']"))
        .await?;
    support::context_click_element(driver, &inactive).await?;
    let menu = tab_actions_menu(driver).await?;
    support::evidence::capture(driver, "viewer-tab-actions-menu", true).await?;
    menu.find(By::Css("[aria-keyshortcuts='Alt+p']"))
        .await?
        .click()
        .await?;
    wait_for_pin_count(driver, 1).await?;
    ensure!(
        driver.current_url().await? == active_url,
        "pinning an inactive tab activated it"
    );

    press_tab_shortcut(driver, Key::Alt, "p").await?;
    wait_for_pin_count(driver, 2).await?;
    press_tab_shortcut(driver, Key::Alt, "p").await?;
    wait_for_pin_count(driver, 1).await?;
    press_tab_shortcut(driver, Key::Alt, "o").await?;
    wait_for_tab_count(driver, 2).await?;
    wait_for_pin_count(driver, 1).await?;
    press_tab_shortcut(driver, Key::Control, "w").await?;
    wait_for_tab_count(driver, 1).await?;
    wait_for_pin_count(driver, 1).await?;

    close_last_pinned_tab(driver).await
}

async fn close_last_pinned_tab(driver: &WebDriver) -> Result<()> {
    let active = driver
        .find(By::Css("[role='tab'][aria-selected='true']"))
        .await?;
    support::context_click_element(driver, &active).await?;
    let menu = tab_actions_menu(driver).await?;
    ensure!(
        !menu
            .find(By::Css("[aria-keyshortcuts='Control+w']"))
            .await?
            .is_enabled()
            .await?,
        "pinned tab offered Close tab"
    );
    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await?;
    menu.wait_until().not_displayed().await?;
    driver
        .action_chain()
        .key_down(Key::Shift)
        .send_keys(Key::F10)
        .key_up(Key::Shift)
        .perform()
        .await?;
    tab_actions_menu(driver).await?;
    driver.action_chain().send_keys(Key::Down).perform().await?;
    ensure!(
        driver
            .active_element()
            .await?
            .attr("aria-keyshortcuts")
            .await?
            .as_deref()
            == Some("Alt+o"),
        "menu keyboard navigation did not skip disabled Close tab"
    );
    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await?;
    support::context_click_element(driver, &active).await?;
    let menu = tab_actions_menu(driver).await?;
    driver
        .find(By::Css("a[aria-label='Projects']"))
        .await?
        .click()
        .await?;
    menu.wait_until().not_displayed().await?;

    active.click().await?;
    close_last_overflow_tab(driver).await
}

async fn close_last_overflow_tab(driver: &WebDriver) -> Result<()> {
    driver.set_window_rect(20, 20, 300, WINDOW_HEIGHT).await?;
    let overflow = wait_for_overflow_trigger(driver).await?;
    overflow.click().await?;
    let tab = driver
        .find(By::Css(
            "#viewer-tab-overflow-menu [aria-controls='viewer-active-view']",
        ))
        .await?;
    support::context_click_element(driver, &tab).await?;
    tab_actions_menu(driver).await?;
    support::evidence::capture(driver, "viewer-tab-actions-overflow", true).await?;
    press_tab_shortcut(driver, Key::Alt, "p").await?;
    wait_for_pin_count(driver, 0).await?;
    support::context_click_element(driver, &tab).await?;
    tab_actions_menu(driver)
        .await?
        .find(By::Css("[aria-keyshortcuts='Control+w']"))
        .await?
        .click()
        .await?;
    driver
        .query(By::Id("projects-heading"))
        .and_displayed()
        .first()
        .await?;
    driver
        .query(By::Css("#viewer-tab-overflow-menu"))
        .with_text(thirtyfour::stringmatch::StringMatch::new("No open diffs").partial())
        .first()
        .await?;
    Ok(())
}

async fn tab_actions_menu(driver: &WebDriver) -> Result<WebElement> {
    Ok(driver
        .query(By::Css(
            "[role='menu'][aria-label='Tab actions']:popover-open",
        ))
        .and_displayed()
        .wait(wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await?)
}

async fn press_tab_shortcut(driver: &WebDriver, modifier: Key, key: &str) -> Result<()> {
    driver
        .action_chain()
        .key_down(modifier.clone())
        .send_keys(key)
        .key_up(modifier)
        .perform()
        .await?;
    Ok(())
}

async fn wait_for_pin_count(driver: &WebDriver, count: usize) -> Result<()> {
    wait::until("expected pinned tabs", wait::ASSERTION_TIMEOUT, || async {
        Ok((driver
            .find_all(By::Css("button[aria-label^='Unpin ']"))
            .await?
            .len()
            == count)
            .then_some(()))
    })
    .await
}

async fn wait_for_tab_count(driver: &WebDriver, count: usize) -> Result<()> {
    wait::until("expected open tabs", wait::ASSERTION_TIMEOUT, || async {
        Ok((driver.find_all(By::Css("[role='tab']")).await?.len() == count).then_some(()))
    })
    .await
}

async fn run_tab_overflow(session: &mut support::session::TestSession) -> Result<()> {
    let fixture = TabOverflowFixture::create(session.data_root())?;
    verify_rail_responsiveness(session.driver(), &fixture).await?;
    verify_overflow_menu(session.driver(), &fixture).await
}

async fn verify_rail_responsiveness(
    driver: &WebDriver,
    fixture: &TabOverflowFixture,
) -> Result<()> {
    driver
        .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
        .await?;
    fixture.forward_initial()?;
    let (initial_last_repository, initial_last_marker) = fixture
        .initial_last_identity()
        .context("tab overflow fixture has an initial final snapshot")?;
    support::wait_for_active_diff(driver, initial_last_repository, initial_last_marker).await?;
    let initial_url = driver.current_url().await?;
    let initial_content_width = assert_tab_rail_presentation(driver).await?;
    let (pressed_repository, pressed_marker) = fixture
        .first_identity()
        .context("tab overflow fixture has a first snapshot")?;
    assert_pointer_down_activation(driver, pressed_repository, pressed_marker).await?;
    let pressed_url = driver.current_url().await?;
    ensure!(
        initial_url != pressed_url,
        "selecting another tab did not change its route"
    );
    driver.back().await?;
    support::wait_for_active_diff(driver, initial_last_repository, initial_last_marker).await?;
    ensure!(
        driver.current_url().await? == initial_url,
        "Back did not select the preceding diff route"
    );
    driver.forward().await?;
    support::wait_for_active_diff(driver, pressed_repository, pressed_marker).await?;
    ensure!(
        driver.current_url().await? == pressed_url,
        "Forward did not select the next diff route"
    );
    assert_drag_reordering(driver).await?;
    ensure!(
        visible_overflow_trigger(driver).await?.is_none(),
        "the tab rail collapsed while all tabs fit"
    );

    driver
        .set_window_rect(20, 20, NARROW_WIDTH, WINDOW_HEIGHT)
        .await
        .context("narrow the viewer window")?;
    wait_for_overflow_trigger(driver).await?;
    let tablist = driver
        .find(By::Css("[role='tablist'][aria-label='Open diffs']"))
        .await
        .context("find the measured tab rail")?;
    ensure!(
        !tablist.is_displayed().await?,
        "the full tab rail remained visible after it overflowed"
    );
    ensure!(
        tablist.attr("data-viewer-tab-rail-mode").await?.as_deref() == Some("measurement"),
        "the collapsed tab rail retained its interactive representation"
    );
    ensure!(
        tablist
            .find_all(By::Css("button[role='tab']"))
            .await?
            .is_empty(),
        "the collapsed tab rail retained hidden interactive tab controls"
    );
    let measured_content_width = tab_rail_content_width(driver).await?;
    ensure!(
        (measured_content_width - initial_content_width).abs() <= 1.0,
        "the lightweight tab measurement changed the rail width: interactive={initial_content_width}, measurement={measured_content_width}"
    );

    driver
        .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
        .await
        .context("restore the desktop viewer width")?;
    support::wait_for_active_diff(driver, pressed_repository, pressed_marker).await?;
    ensure!(
        visible_overflow_trigger(driver).await?.is_none(),
        "the tab rail stayed collapsed after its initial tabs fit again"
    );
    Ok(())
}

async fn assert_tab_rail_presentation(driver: &WebDriver) -> Result<f64> {
    let rail = driver
        .find(By::Css("[role='tablist'][aria-label='Open diffs']"))
        .await
        .context("find viewer rail")?;
    let tabs = driver
        .find_all(By::Css("button[role='tab']"))
        .await
        .context("list visible viewer tabs")?;
    let first_tab = tabs.first().context("viewer has a first visible tab")?;
    let rail_rect = rail.rect().await.context("measure viewer rail")?;
    let first_tab_rect = first_tab.rect().await.context("measure first viewer tab")?;
    let tab_rail_content_width = tab_rail_content_width(driver).await?;
    ensure!(
        (first_tab_rect.x - rail_rect.x).abs() <= 1.0,
        "the first viewer tab does not align with the diff rail: rail={rail_rect:?}, tab={first_tab_rect:?}"
    );

    let label = first_tab
        .find(By::Css("span.truncate"))
        .await
        .context("find the first viewer tab label")?;
    let label_client_width = numeric_property(&label, "clientWidth").await?;
    let label_scroll_width = numeric_property(&label, "scrollWidth").await?;
    ensure!(
        label_scroll_width <= label_client_width + 1.0,
        "the first viewer tab label truncates despite available rail space: client={label_client_width}, scroll={label_scroll_width}"
    );

    let close = support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_CLOSE).await?;
    ensure!(
        close
            .attr("aria-label")
            .await?
            .is_some_and(|label| label.starts_with("Close ")),
        "the viewer tab close action lost its accessible name"
    );
    driver
        .action_chain()
        .move_to_element_center(&close)
        .perform()
        .await
        .context("hover the first viewer tab close action")?;
    let visual = wait::until(
        "viewer tab danger close treatment",
        wait::ASSERTION_TIMEOUT,
        || async {
            let visual = read_tab_close_visual(driver, &close).await?;
            Ok((visual.opacity >= 0.99).then_some(visual))
        },
    )
    .await?;
    ensure!(
        !visual.has_title,
        "the viewer tab close action exposes a native title tooltip"
    );
    ensure!(
        visual.background_color == "rgb(193, 18, 31)",
        "the viewer tab close action does not use danger red: {visual:?}"
    );
    ensure!(
        visual.stroke == "rgb(255, 255, 255)",
        "the viewer tab close icon is not white: {visual:?}"
    );
    ensure!(
        visual.transition_duration == "0.1s",
        "the viewer tab close animation is not the optimized 100 ms duration: {visual:?}"
    );
    ensure!(
        visual.center_delta_x.abs() <= 0.6 && visual.center_delta_y.abs() <= 0.1,
        "the viewer tab close icon is not optically centered: {visual:?}"
    );
    support::evidence::capture(driver, "viewer-tab-close-hover", true).await?;
    Ok(tab_rail_content_width)
}

async fn tab_rail_content_width(driver: &WebDriver) -> Result<f64> {
    numeric_property(
        &driver
            .find(By::Css("[data-viewer-tab-rail-content='true']"))
            .await
            .context("find the measured viewer tab rail content")?,
        "scrollWidth",
    )
    .await
}

async fn assert_pointer_down_activation(
    driver: &WebDriver,
    repository: &str,
    marker: &str,
) -> Result<()> {
    let tab = tab_for_repository(driver, repository).await?;
    let previously_active = driver
        .find(By::Css("button[role='tab'][aria-selected='true']"))
        .await
        .context("find the previously active viewer tab")?;
    let history_length = browser_history_length(driver).await?;
    driver
        .action_chain()
        .click_and_hold_element(&tab)
        .perform()
        .await
        .with_context(|| format!("press and hold the {repository} viewer tab"))?;
    let activation = async {
        support::wait_for_active_diff(driver, repository, marker).await?;
        ensure!(
            tab.attr("aria-selected").await?.as_deref() == Some("true"),
            "the pressed viewer tab did not become selected before pointer release"
        );
        let motion = read_tab_selection_motion(driver, &tab, &previously_active).await?;
        ensure!(
            motion.active_transition_duration == "0.075s"
                && motion.inactive_transition_duration == "0s",
            "the viewer tab underline does not use an enter-only 75 ms transition: {motion:?}"
        );
        ensure!(
            browser_history_length(driver).await? == history_length + 1,
            "switching tabs did not add exactly one diff route to history"
        );
        Ok(())
    }
    .await;
    driver
        .action_chain()
        .release()
        .perform()
        .await
        .context("release the pressed viewer tab")?;
    activation?;
    ensure!(
        browser_history_length(driver).await? == history_length + 1,
        "releasing the pointer added a duplicate diff route to history"
    );
    Ok(())
}

async fn browser_history_length(driver: &WebDriver) -> Result<u64> {
    driver
        .execute("return window.history.length;", Vec::new())
        .await
        .context("read viewer history length")?
        .convert()
        .context("decode viewer history length")
}

async fn read_tab_selection_motion(
    driver: &WebDriver,
    active: &WebElement,
    inactive: &WebElement,
) -> Result<TabSelectionMotion> {
    driver
        .execute(
            r#"
                const indicator = (tab) => tab.parentElement.querySelector(
                    "[data-viewer-tab-selection-indicator]",
                );
                return {
                    activeTransitionDuration:
                        getComputedStyle(indicator(arguments[0])).transitionDuration,
                    inactiveTransitionDuration:
                        getComputedStyle(indicator(arguments[1])).transitionDuration,
                };
            "#,
            vec![
                active.to_json().context("encode active viewer tab")?,
                inactive.to_json().context("encode inactive viewer tab")?,
            ],
        )
        .await
        .context("inspect viewer tab selection motion")?
        .convert()
        .context("decode viewer tab selection motion")
}

async fn assert_drag_reordering(driver: &WebDriver) -> Result<()> {
    let rail = driver
        .find(By::Css("[data-viewer-tab-rail-content='true']"))
        .await
        .context("find the tab rail for pointer reordering")?;
    assert_pointer_drag_reordering(driver, &rail, TabDragAxis::Horizontal, 0).await
}

async fn assert_pointer_drag_reordering(
    driver: &WebDriver,
    container: &WebElement,
    axis: TabDragAxis,
    source_index: usize,
) -> Result<()> {
    let initial = read_tab_drag_visual(driver, container).await?;
    let source = initial
        .tabs
        .get(source_index)
        .context("viewer has a pointer drag source")?;
    let target = initial
        .tabs
        .get(source_index + 2)
        .context("viewer has two following tabs for pointer reordering")?;
    let destination = match axis {
        TabDragAxis::Horizontal => (target.center_x + 8, source.center_y + 80),
        TabDragAxis::Vertical => (source.center_x + 16, target.center_y + 8),
    };
    let original_order: Vec<_> = initial.tabs.iter().map(|tab| tab.id.as_str()).collect();
    let mut expected_order = original_order.clone();
    expected_order[source_index..=source_index + 2].rotate_left(1);

    drag_tab_to_proposed_position(driver, container, &initial, source_index, axis, destination)
        .await?;
    let evidence_name = match axis {
        TabDragAxis::Horizontal => "viewer-tab-drag-lifted",
        TabDragAxis::Vertical => "viewer-tab-overflow-drag-lifted",
    };
    support::evidence::capture(driver, evidence_name, true).await?;
    if matches!(axis, TabDragAxis::Horizontal) {
        driver
            .action_chain()
            .send_keys(Key::Escape)
            .perform()
            .await
            .context("cancel the tab drag with Escape while holding the pointer")?;
        wait_for_tab_drag_cleanup(driver, container, &original_order).await?;
        driver
            .action_chain()
            .move_by_offset(16, 12)
            .release()
            .perform()
            .await
            .context("move and release the pointer after cancelling the tab drag")?;
        wait_for_tab_drag_cleanup(driver, container, &original_order).await?;
        drag_tab_to_proposed_position(driver, container, &initial, source_index, axis, destination)
            .await?;
    }
    driver
        .action_chain()
        .release()
        .perform()
        .await
        .context("drop the viewer tab at the proposed position")?;
    wait_for_tab_drag_cleanup(driver, container, &expected_order).await
}

async fn drag_tab_to_proposed_position(
    driver: &WebDriver,
    container: &WebElement,
    initial: &TabDragVisual,
    source_index: usize,
    axis: TabDragAxis,
    destination: (i32, i32),
) -> Result<()> {
    let source = &initial.tabs[source_index];
    driver
        .action_chain()
        .move_to(i64::from(source.center_x), i64::from(source.center_y))
        .click_and_hold()
        .perform()
        .await
        .context("press the viewer tab for pointer dragging")?;
    let start = match axis {
        TabDragAxis::Horizontal => (source.center_x + 12, source.center_y + 80),
        TabDragAxis::Vertical => (source.center_x + 16, source.center_y + 12),
    };
    driver
        .action_chain()
        .move_to(i64::from(start.0), i64::from(start.1))
        .perform()
        .await
        .context("pull the held viewer tab away from its initial position")?;
    wait_for_floating_tab(driver, container, source, start).await?;
    driver
        .action_chain()
        .move_to(i64::from(destination.0), i64::from(destination.1))
        .perform()
        .await
        .context("move the held viewer tab past the next two siblings")?;
    wait_for_floating_tab(driver, container, source, destination).await?;
    wait_for_tab_drag_preview(driver, container, initial, source_index, axis).await
}

async fn wait_for_floating_tab(
    driver: &WebDriver,
    container: &WebElement,
    source: &TabDragItemVisual,
    pointer: (i32, i32),
) -> Result<()> {
    let expected_x = source.x + f64::from(pointer.0 - source.center_x);
    let expected_y = source.y + f64::from(pointer.1 - source.center_y);
    let visual = wait::until(
        "floating viewer tab following both pointer coordinates",
        wait::ASSERTION_TIMEOUT,
        || async {
            let visual = read_tab_drag_visual(driver, container).await?;
            let [floating] = visual.floating.as_slice() else {
                return Ok(None);
            };
            let follows_pointer =
                (floating.x - expected_x).abs() <= 1.0 && (floating.y - expected_y).abs() <= 1.0;
            Ok(follows_pointer.then_some(visual))
        },
    )
    .await?;
    ensure!(
        visual.tabs.iter().filter(|tab| tab.dragging).count() == 1
            && visual
                .tabs
                .iter()
                .any(|tab| tab.id == source.id && tab.dragging),
        "only the source wrapper should be marked as dragging: {visual:?}"
    );
    let floating = &visual.floating[0];
    ensure!(
        floating.inert && floating.popover_manual && floating.popover_open,
        "the floating viewer tab is not an inert manual popover in the top layer: {floating:?}"
    );
    Ok(())
}

async fn wait_for_tab_drag_preview(
    driver: &WebDriver,
    container: &WebElement,
    initial: &TabDragVisual,
    source_index: usize,
    axis: TabDragAxis,
) -> Result<()> {
    let source = &initial.tabs[source_index];
    let next = &initial.tabs[source_index + 1];
    let displacement = match axis {
        TabDragAxis::Horizontal => source.x - next.x,
        TabDragAxis::Vertical => source.y - next.y,
    };
    let visual = wait::until(
        "sibling transforms opening the proposed tab position before drop",
        wait::ASSERTION_TIMEOUT,
        || async {
            let visual = read_tab_drag_visual(driver, container).await?;
            ensure!(
                visual
                    .tabs
                    .iter()
                    .map(|tab| &tab.id)
                    .eq(initial.tabs.iter().map(|tab| &tab.id)),
                "the tab order changed before pointer release: {visual:?}"
            );
            let preview_ready = visual.tabs.iter().enumerate().all(|(index, tab)| {
                let offset = if index > source_index && index <= source_index + 2 {
                    displacement
                } else {
                    0.0
                };
                let (expected_x, expected_y) = match axis {
                    TabDragAxis::Horizontal => (offset, 0.0),
                    TabDragAxis::Vertical => (0.0, offset),
                };
                (tab.transform_x - expected_x).abs() <= 1.0
                    && (tab.transform_y - expected_y).abs() <= 1.0
            });
            Ok(preview_ready.then_some(visual))
        },
    )
    .await?;
    for tab in visual.tabs.iter().filter(|tab| tab.id != source.id) {
        let expected_motion = if visual.reduced_motion {
            tab.transition_property == "none"
        } else {
            tab.transition_property
                .split(',')
                .any(|property| property.trim() == "transform")
                && tab.transition_duration == "0.16s"
        };
        ensure!(
            expected_motion,
            "sibling {} must transition transforms for 160 ms or disable transitions under reduced motion: {tab:?}",
            tab.id
        );
    }
    Ok(())
}

async fn wait_for_tab_drag_cleanup(
    driver: &WebDriver,
    container: &WebElement,
    expected_order: &[&str],
) -> Result<()> {
    let settling_started = Instant::now();
    wait::until(
        "expected tab order with no floating tab or drag transforms",
        wait::ASSERTION_TIMEOUT,
        || async {
            if settling_started.elapsed() < Duration::from_millis(200) {
                return Ok(None);
            }
            let visual = read_tab_drag_visual(driver, container).await?;
            let order_matches = visual
                .tabs
                .iter()
                .map(|tab| tab.id.as_str())
                .eq(expected_order.iter().copied());
            let clean = visual.floating.is_empty()
                && visual.tabs.iter().all(|tab| {
                    !tab.dragging && tab.transform_x.abs() <= 1.0 && tab.transform_y.abs() <= 1.0
                });
            Ok((order_matches && clean).then_some(()))
        },
    )
    .await
}

async fn read_tab_drag_visual(driver: &WebDriver, container: &WebElement) -> Result<TabDragVisual> {
    driver
        .execute(
            r#"
                const floatingSelector = "[data-viewer-tab-floating='true']";
                const tabs = [...arguments[0].querySelectorAll("[data-viewer-tab-id]")]
                    .filter((tab) => !tab.closest(floatingSelector))
                    .map((tab) => {
                        const box = tab.getBoundingClientRect();
                        const style = getComputedStyle(tab);
                        const transform = style.transform === "none"
                            ? new DOMMatrixReadOnly()
                            : new DOMMatrixReadOnly(style.transform);
                        return {
                            id: tab.dataset.viewerTabId,
                            x: box.x,
                            y: box.y,
                            centerX: Math.floor(box.x + box.width / 2),
                            centerY: Math.floor(box.y + box.height / 2),
                            dragging: tab.dataset.dragState === "dragging",
                            transformX: transform.m41,
                            transformY: transform.m42,
                            transitionProperty: style.transitionProperty,
                            transitionDuration: style.transitionDuration,
                        };
                    });
                const floating = [...document.querySelectorAll(floatingSelector)]
                    .map((tab) => {
                        const box = tab.getBoundingClientRect();
                        return {
                            x: box.x,
                            y: box.y,
                            inert: tab.inert,
                            popoverManual: tab.getAttribute("popover") === "manual",
                            popoverOpen: tab.matches(":popover-open"),
                        };
                    });
                return {
                    tabs,
                    floating,
                    reducedMotion: matchMedia("(prefers-reduced-motion: reduce)").matches,
                };
            "#,
            vec![
                container
                    .to_json()
                    .context("encode the viewer tab container")?,
            ],
        )
        .await
        .context("inspect pointer drag geometry and presentation")?
        .convert()
        .context("decode pointer drag geometry and presentation")
}

async fn tab_for_repository(driver: &WebDriver, repository: &str) -> Result<WebElement> {
    for tab in driver
        .find_all(By::Css("button[role='tab']"))
        .await
        .context("list viewer tabs")?
    {
        if tab
            .attr("title")
            .await?
            .is_some_and(|title| title.contains(repository))
        {
            return Ok(tab);
        }
    }
    anyhow::bail!("viewer tab for {repository} is unavailable")
}

async fn read_tab_close_visual(driver: &WebDriver, close: &WebElement) -> Result<TabCloseVisual> {
    driver
        .execute(
            r#"
                const close = arguments[0];
                const circle = close.children[0];
                const icon = close.querySelector("svg");
                const circleBox = circle.getBoundingClientRect();
                const iconBox = icon.getBoundingClientRect();
                const circleStyle = getComputedStyle(circle);
                const iconStyle = getComputedStyle(icon);
                return {
                    backgroundColor: circleStyle.backgroundColor,
                    centerDeltaX:
                        iconBox.left + iconBox.width / 2 -
                        (circleBox.left + circleBox.width / 2),
                    centerDeltaY:
                        iconBox.top + iconBox.height / 2 -
                        (circleBox.top + circleBox.height / 2),
                    hasTitle: close.hasAttribute("title"),
                    opacity: Number(circleStyle.opacity),
                    stroke: iconStyle.stroke,
                    transitionDuration: circleStyle.transitionDuration,
                };
            "#,
            vec![close.to_json().context("encode viewer tab close action")?],
        )
        .await
        .context("inspect viewer tab close presentation")?
        .convert()
        .context("decode viewer tab close presentation")
}

async fn verify_overflow_menu(driver: &WebDriver, fixture: &TabOverflowFixture) -> Result<()> {
    driver
        .set_window_rect(20, 20, NARROW_WIDTH, WINDOW_HEIGHT)
        .await
        .context("narrow the viewer window for the overflow menu")?;
    wait_for_overflow_trigger(driver).await?;
    fixture.forward_remaining()?;
    let (last_repository, last_marker) = fixture
        .last_identity()
        .context("tab overflow fixture has a final snapshot")?;
    wait_for_collapsed_active_diff(driver, last_repository, last_marker).await?;
    let trigger = wait_for_overflow_trigger(driver).await?;
    trigger
        .click()
        .await
        .context("open the tab overflow menu")?;
    let menu = support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_OVERFLOW_MENU).await?;
    assert_menu_geometry(driver, &trigger, &menu).await?;
    ensure!(
        menu.css_value("animation-name").await? == "none",
        "the overflow menu delays its content with an entrance animation"
    );
    assert_menu_contents(&menu, fixture.len()).await?;
    support::evidence::capture(driver, "viewer-tab-overflow-menu", true).await?;
    assert_pointer_drag_reordering(driver, &menu, TabDragAxis::Vertical, 3).await?;
    let menu_tabs = assert_menu_contents(&menu, fixture.len()).await?;

    let (first_repository, first_marker) = fixture
        .first_identity()
        .context("tab overflow fixture has a first snapshot")?;
    menu_tabs
        .get(2)
        .context("overflow menu retained the moved tab in third position")?
        .click()
        .await
        .context("activate the moved overflowed tab")?;
    wait_for_collapsed_active_diff(driver, first_repository, first_marker).await?;
    menu.wait_until().not_displayed().await?;
    verify_overflow_closing(driver, fixture.len()).await
}

async fn verify_overflow_closing(driver: &WebDriver, tab_count: usize) -> Result<()> {
    wait_for_overflow_trigger(driver).await?.click().await?;
    let menu = support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_OVERFLOW_MENU).await?;
    menu.wait_until().displayed().await?;
    let active_close = menu
        .find(By::Css(
            "li[data-active='true'] button[aria-label^='Close ']",
        ))
        .await?;
    active_close.click().await?;
    wait_for_menu_tab_count(&menu, tab_count - 1).await?;
    ensure!(
        menu.is_displayed().await?,
        "closing the active tab dismissed the overflow menu"
    );

    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await?;
    menu.wait_until().not_displayed().await?;
    wait_for_overflow_trigger(driver).await?.click().await?;
    menu.wait_until().displayed().await?;
    driver
        .action_chain()
        .move_to(10, i64::from(WINDOW_HEIGHT - 100))
        .click()
        .perform()
        .await?;
    menu.wait_until().not_displayed().await?;
    wait_for_overflow_trigger(driver).await?.click().await?;
    menu.wait_until().displayed().await?;

    for remaining in (0..tab_count - 1).rev() {
        menu.find(By::Css("button[aria-label^='Close ']"))
            .await?
            .click()
            .await?;
        wait_for_menu_tab_count(&menu, remaining).await?;
        ensure!(
            menu.is_displayed().await?,
            "closing a tab dismissed the overflow menu with {remaining} tabs left"
        );
        if remaining == 1 {
            support::evidence::capture(driver, "viewer-tab-overflow-one-remaining", true).await?;
        }
    }
    ensure!(
        menu.text().await?.contains("No open diffs"),
        "the empty overflow menu lost its empty state"
    );
    support::evidence::capture(driver, "viewer-tab-overflow-empty", true).await?;
    driver
        .action_chain()
        .send_keys(Key::Escape)
        .perform()
        .await?;
    wait::until(
        "overflow trigger removed after empty menu dismissal",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok(visible_overflow_trigger(driver)
                .await?
                .is_none()
                .then_some(()))
        },
    )
    .await
}

async fn wait_for_menu_tab_count(menu: &WebElement, expected: usize) -> Result<()> {
    wait::until(
        "remaining overflow tabs",
        wait::ASSERTION_TIMEOUT,
        || async {
            let tabs = menu
                .find_all(By::Css("li > button[aria-controls='viewer-active-view']"))
                .await?;
            Ok((tabs.len() == expected).then_some(()))
        },
    )
    .await
}

async fn assert_menu_geometry(
    driver: &WebDriver,
    trigger: &WebElement,
    menu: &WebElement,
) -> Result<()> {
    let trigger_rect = trigger.rect().await.context("measure the menu trigger")?;
    let menu_rect = menu.rect().await.context("measure the overflow menu")?;
    let window = driver
        .get_window_rect()
        .await
        .context("measure the narrow viewer window")?;
    let viewport_height = u32::try_from(window.height).context("convert viewport height")?;
    ensure!(
        menu_rect.y >= trigger_rect.y + trigger_rect.height,
        "the overflow menu rendered above its trigger: trigger={trigger_rect:?}, menu={menu_rect:?}"
    );
    ensure!(
        menu_rect.y + menu_rect.height <= f64::from(viewport_height),
        "the overflow menu extended below the viewport: window={window:?}, menu={menu_rect:?}"
    );
    Ok(())
}

async fn assert_menu_contents(menu: &WebElement, expected_count: usize) -> Result<Vec<WebElement>> {
    let menu_tabs = menu
        .find_all(By::Css("li > button[aria-controls='viewer-active-view']"))
        .await
        .context("list tabs in the overflow menu")?;
    ensure!(
        menu_tabs.len() == expected_count,
        "the overflow menu exposed {} tabs instead of {}",
        menu_tabs.len(),
        expected_count
    );
    let menu_scroll_area = menu
        .find(By::Css("[class~='overflow-y-auto']"))
        .await
        .context("find the overflow menu scroll area")?;
    let client_height = numeric_property(&menu_scroll_area, "clientHeight").await?;
    let scroll_height = numeric_property(&menu_scroll_area, "scrollHeight").await?;
    let scroll_top = numeric_property(&menu_scroll_area, "scrollTop").await?;
    ensure!(
        scroll_height > client_height && scroll_top == 0.0,
        "the overflow menu did not start at the first tab in a scrollable viewport: client={client_height}, scroll={scroll_height}, top={scroll_top}"
    );
    ensure!(
        menu_tabs
            .first()
            .context("overflow menu has a first tab")?
            .is_displayed()
            .await?,
        "the first overflowed tab was not immediately accessible"
    );
    Ok(menu_tabs)
}

async fn numeric_property(element: &WebElement, property: &'static str) -> Result<f64> {
    let value = element
        .prop(property)
        .await?
        .with_context(|| format!("read {property} from the overflow menu"))?;
    value
        .parse()
        .with_context(|| format!("parse {property} from the overflow menu"))
}

async fn wait_for_overflow_trigger(driver: &WebDriver) -> Result<WebElement> {
    wait::until(
        "visible tab overflow trigger",
        wait::ASSERTION_TIMEOUT,
        || async { visible_overflow_trigger(driver).await },
    )
    .await
}

async fn visible_overflow_trigger(driver: &WebDriver) -> Result<Option<WebElement>> {
    let triggers = driver
        .find_all(By::Css(test_ids::VIEWER_TAB_OVERFLOW_TRIGGER.selector()))
        .await?;
    for trigger in triggers {
        if trigger.is_displayed().await? {
            return Ok(Some(trigger));
        }
    }
    Ok(None)
}

async fn wait_for_collapsed_active_diff(
    driver: &WebDriver,
    repository: &str,
    marker: &str,
) -> Result<()> {
    wait::until(
        &format!("collapsed {repository} diff containing {marker}"),
        wait::ASSERTION_TIMEOUT,
        || async {
            let Some(trigger) = visible_overflow_trigger(driver).await? else {
                return Ok(None);
            };
            let label = trigger.attr("aria-label").await?.unwrap_or_default();
            if !label.contains(repository) {
                return Ok(None);
            }
            let main = driver
                .query(By::Css("main"))
                .and_displayed()
                .first()
                .await?;
            Ok((main.is_displayed().await? && main.text().await?.contains(marker)).then_some(()))
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_rename_edits_in_place_and_survives_reopen_and_restart() -> Result<()> {
    support::run_test("viewer-snapshot-rename", |session| {
        Box::pin(run_snapshot_rename(session))
    })
    .await
}

async fn snapshot_name_editor(driver: &WebDriver, target: &WebElement) -> Result<WebElement> {
    support::context_click_element(driver, target).await?;
    tab_actions_menu(driver)
        .await?
        .find(By::XPath(".//button[normalize-space(.)='Rename snapshot']"))
        .await?
        .click()
        .await?;
    Ok(driver
        .query(By::Css("input[aria-label='Snapshot name']"))
        .and_displayed()
        .first()
        .await?)
}

async fn wait_for_snapshot_name(driver: &WebDriver, name: &str) -> Result<WebElement> {
    Ok(driver
        .query(By::Css(format!("[role='tab'][title='{name}']")))
        .and_displayed()
        .first()
        .await?)
}

async fn run_snapshot_rename(session: &mut support::session::TestSession) -> Result<()> {
    let fixture =
        support::fixture::OneShotFixture::create_named(session.data_root(), "rename-snapshot")?;
    fixture.forward()?;
    let driver = session.driver();
    driver
        .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
        .await?;
    support::wait_for_active_diff(driver, "rename-snapshot", "alpha-one-shot-marker").await?;
    let tab = driver
        .find(By::Css("[role='tab'][aria-selected='true']"))
        .await?;
    let title = tab.attr("title").await?.context("original title")?;
    let original_rect = tab.rect().await?;
    let editor = snapshot_name_editor(driver, &tab).await?;
    ensure!(
        editor.value().await?.as_deref() == Some(""),
        "generated name was prefilled"
    );
    ensure!(
        driver.active_element().await?.element_id() == editor.element_id(),
        "editor did not receive focus"
    );
    let editor_rect = editor.rect().await?;
    ensure!(
        (editor_rect.y - original_rect.y).abs() < 10.0,
        "editor moved outside the tab title"
    );
    editor.send_keys("Discard this").await?;
    editor.send_keys(Key::Escape).await?;
    let tab = wait_for_snapshot_name(driver, &title).await?;
    let editor = snapshot_name_editor(driver, &tab).await?;
    ensure!(
        editor.value().await?.as_deref() == Some(""),
        "cancel retained a draft"
    );
    editor.send_keys("Auth review").await?;
    support::evidence::capture(driver, "viewer-snapshot-rename-inline", true).await?;
    editor.send_keys(Key::Enter).await?;
    let tab = wait_for_snapshot_name(driver, "Auth review").await?;
    let editor = snapshot_name_editor(driver, &tab).await?;
    ensure!(
        editor.value().await?.as_deref() == Some("Auth review"),
        "custom name was not prefilled"
    );
    editor.send_keys(Key::Escape).await?;
    support::selectors::by_test_id(driver, test_ids::VIEWER_TAB_CLOSE)
        .await?
        .click()
        .await?;
    support::selectors::by_test_id(driver, test_ids::VIEWER_HISTORY_OPEN)
        .await?
        .click()
        .await?;
    support::selectors::by_test_id(driver, test_ids::HISTORY_ENTRY_OPEN)
        .await?
        .click()
        .await?;
    let tab = wait_for_snapshot_name(driver, "Auth review").await?;
    support::context_click_element(driver, &tab).await?;
    tab_actions_menu(driver)
        .await?
        .find(By::Css("[aria-keyshortcuts='Alt+p']"))
        .await?
        .click()
        .await?;
    wait_for_pin_count(driver, 1).await?;
    rename_snapshot_from_overflow(driver).await?;
    session.restart_server().await?;
    session
        .driver()
        .set_window_rect(20, 20, DESKTOP_WIDTH, WINDOW_HEIGHT)
        .await?;
    wait_for_snapshot_name(session.driver(), "Release review").await?;
    Ok(())
}

async fn rename_snapshot_from_overflow(driver: &WebDriver) -> Result<()> {
    driver.set_window_rect(20, 20, 300, WINDOW_HEIGHT).await?;
    wait_for_overflow_trigger(driver).await?.click().await?;
    let tab = driver
        .find(By::Css(
            "#viewer-tab-overflow-menu .viewer-tab-menu-trigger",
        ))
        .await?;
    let editor = snapshot_name_editor(driver, &tab).await?;
    driver
        .action_chain()
        .key_down(Key::Control)
        .send_keys("a")
        .key_up(Key::Control)
        .send_keys("Release review")
        .perform()
        .await?;
    support::evidence::capture(driver, "viewer-snapshot-rename-overflow", true).await?;
    editor
        .send_keys(Key::Enter)
        .await
        .context("save the name from the overflow editor")?;
    driver
        .query(By::Css("input[aria-label='Snapshot name']"))
        .not_exists()
        .await?;
    wait_for_overflow_trigger(driver).await?.click().await?;
    Ok(())
}
