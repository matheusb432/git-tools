use anyhow::{Context as _, Result, ensure};
use gtl_web_contracts::test_ids;
use serde::Deserialize;
use thirtyfour::{By, WebDriver, WebElement};

use crate::support::{self, fixture::TabOverflowFixture, wait};

const DESKTOP_WIDTH: u32 = 1200;
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OverflowMenuMotion {
    animation_duration: String,
    animation_name: String,
    keyframes: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn tab_rail_collapses_only_while_its_tabs_overflow() -> Result<()> {
    support::run_test("viewer-tab-overflow", |session| {
        Box::pin(run_tab_overflow(session))
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
    fixture.forward_initial()?;
    let (initial_last_repository, initial_last_marker) = fixture
        .initial_last_identity()
        .context("tab overflow fixture has an initial final snapshot")?;
    support::wait_for_active_diff(driver, initial_last_repository, initial_last_marker).await?;
    let initial_content_width = assert_tab_rail_presentation(driver).await?;
    let (pressed_repository, pressed_marker) = fixture
        .first_identity()
        .context("tab overflow fixture has a first snapshot")?;
    assert_pointer_down_activation(driver, pressed_repository, pressed_marker).await?;
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
    let navigation = driver
        .find(By::Css("nav[aria-label='Viewer navigation']"))
        .await
        .context("find viewer navigation")?;
    let tabs = driver
        .find_all(By::Css("button[role='tab']"))
        .await
        .context("list visible viewer tabs")?;
    let first_tab = tabs.first().context("viewer has a first visible tab")?;
    let navigation_rect = navigation
        .rect()
        .await
        .context("measure viewer navigation")?;
    let first_tab_rect = first_tab.rect().await.context("measure first viewer tab")?;
    let tab_rail_content_width = tab_rail_content_width(driver).await?;
    ensure!(
        first_tab_rect.x < navigation_rect.x,
        "the first viewer tab does not overlap the shell edge: navigation={navigation_rect:?}, tab={first_tab_rect:?}"
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
            browser_history_length(driver).await? == history_length,
            "switching tabs while already in the workspace pushed a redundant history entry"
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
    activation
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
    let tabs = driver
        .find_all(By::Css("button[role='tab']"))
        .await
        .context("list viewer tabs before reordering")?;
    let source = tabs.first().context("viewer has a drag source tab")?;
    let target = tabs.get(2).context("viewer has a drag target tab")?;
    let titles = tab_titles(&tabs).await?;
    let expected = vec![
        titles[1].clone(),
        titles[2].clone(),
        titles[0].clone(),
        titles[3].clone(),
    ];
    driver
        .action_chain()
        .move_to_element_center(source)
        .click_and_hold()
        .perform()
        .await
        .context("hold the first viewer tab for dragging")?;
    driver
        .action_chain()
        .move_by_offset(12, 0)
        .perform()
        .await
        .context("start dragging the first viewer tab")?;
    wait::until(
        "started viewer tab drag",
        wait::ASSERTION_TIMEOUT,
        || async {
            let source_state = source
                .find(By::XPath(".."))
                .await?
                .attr("data-drag-state")
                .await?;
            Ok((source_state.as_deref() == Some("dragging")).then_some(()))
        },
    )
    .await?;
    // WebKit's driver starts the native drag but does not carry its transfer data through
    // target-directed pointer moves. Exercise the target half with an equivalent transfer.
    dispatch_tab_drag_over(driver, source, target).await?;
    wait::until(
        "viewer tab drop target",
        wait::ASSERTION_TIMEOUT,
        || async {
            let target_state = target
                .find(By::XPath(".."))
                .await?
                .attr("data-drag-state")
                .await?;
            Ok((target_state.as_deref() == Some("drop-after")).then_some(()))
        },
    )
    .await?;
    driver
        .action_chain()
        .move_by_offset(0, 80)
        .perform()
        .await
        .context("move the native drag away from the tab controls")?;
    driver
        .action_chain()
        .release()
        .perform()
        .await
        .context("end the native viewer tab drag")?;
    dispatch_tab_drop(driver, source, target).await?;
    wait::until("reordered viewer tabs", wait::ASSERTION_TIMEOUT, || async {
        let tabs = driver.find_all(By::Css("button[role='tab']")).await?;
        Ok((tab_titles(&tabs).await? == expected).then_some(()))
    })
    .await
}

async fn dispatch_tab_drag_over(
    driver: &WebDriver,
    source: &WebElement,
    target: &WebElement,
) -> Result<()> {
    let accepted: bool = driver
        .execute(
            r#"
                const [source, target] = arguments;
                const transfer = new DataTransfer();
                transfer.effectAllowed = "move";
                transfer.dropEffect = "move";
                transfer.setData(
                    "text/plain",
                    source.id.slice("viewer-tab-".length),
                );
                window.__gtlViewerTabDragTransfer = transfer;
                const dropZone = target.parentElement.querySelector(
                    '[data-viewer-tab-drop-placement="after"]',
                );
                if (!dropZone) {
                    return false;
                }
                const box = dropZone.getBoundingClientRect();
                const event = new DragEvent("dragover", {
                    bubbles: true,
                    cancelable: true,
                    clientX: box.right - 2,
                    clientY: box.top + box.height / 2,
                    dataTransfer: transfer,
                });
                return dropZone.dispatchEvent(event) === false;
            "#,
            vec![
                source.to_json().context("encode viewer tab drag source")?,
                target.to_json().context("encode viewer tab drag target")?,
            ],
        )
        .await
        .context("drag the first viewer tab over the third")?
        .convert()
        .context("decode whether the viewer accepted the drag")?;
    ensure!(accepted, "the viewer tab rail did not accept the drag");
    Ok(())
}

async fn dispatch_tab_drop(
    driver: &WebDriver,
    source: &WebElement,
    target: &WebElement,
) -> Result<()> {
    let accepted: bool = driver
        .execute(
            r#"
                const [source, target] = arguments;
                const transfer = window.__gtlViewerTabDragTransfer;
                const dropZone = target.parentElement.querySelector(
                    '[data-viewer-tab-drop-placement="after"]',
                );
                if (!dropZone) {
                    return false;
                }
                const box = dropZone.getBoundingClientRect();
                const event = new DragEvent("drop", {
                    bubbles: true,
                    cancelable: true,
                    clientX: box.right - 2,
                    clientY: box.top + box.height / 2,
                    dataTransfer: transfer,
                });
                const accepted = dropZone.dispatchEvent(event) === false;
                source.parentElement.dispatchEvent(new DragEvent("dragend", {
                    bubbles: true,
                    dataTransfer: transfer,
                }));
                delete window.__gtlViewerTabDragTransfer;
                return accepted;
            "#,
            vec![
                source.to_json().context("encode viewer tab drag source")?,
                target.to_json().context("encode viewer tab drop target")?,
            ],
        )
        .await
        .context("drop the first viewer tab after the third")?
        .convert()
        .context("decode whether the viewer accepted the drop")?;
    ensure!(accepted, "the viewer tab rail did not accept the drop");
    Ok(())
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

async fn tab_titles(tabs: &[WebElement]) -> Result<Vec<String>> {
    let mut titles = Vec::with_capacity(tabs.len());
    for tab in tabs {
        titles.push(
            tab.attr("title")
                .await?
                .context("viewer tab is missing its label title")?,
        );
    }
    Ok(titles)
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
    let motion = read_overflow_menu_motion(driver, &menu).await?;
    ensure!(
        motion.animation_name == "popover-enter"
            && motion.animation_duration == "0.1s"
            && motion.keyframes.contains("translate")
            && !motion.keyframes.contains("scale"),
        "the overflow menu does not use the optimized translation-only motion: {motion:?}"
    );
    let menu_tabs = assert_menu_contents(&menu, fixture.len()).await?;
    support::evidence::capture(driver, "viewer-tab-overflow-menu", true).await?;

    let (first_repository, first_marker) = fixture
        .first_identity()
        .context("tab overflow fixture has a first snapshot")?;
    menu_tabs
        .get(2)
        .context("overflow menu retained the moved tab in third position")?
        .click()
        .await
        .context("activate the moved overflowed tab")?;
    wait_for_collapsed_active_diff(driver, first_repository, first_marker).await
}

async fn read_overflow_menu_motion(
    driver: &WebDriver,
    menu: &WebElement,
) -> Result<OverflowMenuMotion> {
    driver
        .execute(
            r#"
                const menu = arguments[0];
                const style = getComputedStyle(menu);
                const findKeyframes = (rules, name) => {
                    for (const rule of rules) {
                        if (rule.type === CSSRule.KEYFRAMES_RULE && rule.name === name) {
                            return rule.cssText;
                        }
                        if (rule.cssRules) {
                            const nested = findKeyframes(rule.cssRules, name);
                            if (nested) return nested;
                        }
                    }
                    return "";
                };
                let keyframes = "";
                for (const sheet of document.styleSheets) {
                    try {
                        keyframes = findKeyframes(sheet.cssRules, style.animationName);
                    } catch (_) {}
                    if (keyframes) break;
                }
                return {
                    animationDuration: style.animationDuration,
                    animationName: style.animationName,
                    keyframes,
                };
            "#,
            vec![menu.to_json().context("encode viewer tab overflow menu")?],
        )
        .await
        .context("inspect viewer tab overflow menu motion")?
        .convert()
        .context("decode viewer tab overflow menu motion")
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
            let main = driver.find(By::Css("main")).await?;
            Ok((main.is_displayed().await? && main.text().await?.contains(marker)).then_some(()))
        },
    )
    .await
}
