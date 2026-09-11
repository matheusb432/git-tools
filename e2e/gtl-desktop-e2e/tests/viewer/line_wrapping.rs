use anyhow::{Result, ensure};
use gtl_web_contracts::test_ids;
use serde::Deserialize;
use thirtyfour::{By, WebDriver, components::SelectElement};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_selects_persistent_line_wrapping_for_both_layouts() -> Result<()> {
    support::run_test("viewer-line-wrapping", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create_named(session.data_root(), "wrapping-alpha")?;
            fixture.forward()?;
            support::wait_for_active_diff(
                session.driver(),
                "wrapping-alpha",
                "alpha-one-shot-marker",
            )
            .await?;
            assert_wrapping(session.driver(), false).await?;
            assert_sticky_horizontal_scrollbar(session.driver()).await?;
            let navigation = session
                .driver()
                .find(By::Css("nav[aria-label='Viewer navigation']"))
                .await?
                .rect()
                .await?;
            let tab = session
                .driver()
                .find(By::Css("[role='tab']"))
                .await?
                .rect()
                .await?;
            ensure!(
                (navigation.y - tab.y).abs() < 1.0,
                "tabs have an inset above them: nav={navigation:?}, tab={tab:?}"
            );
            verify_layouts(session.driver(), &fixture).await?;
            ensure!(
                std::fs::read_to_string(session.data_root().join("config.toml"))?
                    .contains("wrap_lines = false"),
                "explicit no-wrap choice was not persisted"
            );
            session.restart().await?;
            fixture.forward()?;
            support::wait_for_active_diff(
                session.driver(),
                "wrapping-alpha",
                "alpha-one-shot-marker",
            )
            .await?;
            assert_wrapping(session.driver(), false).await?;
            Ok(())
        })
    })
    .await
}

async fn assert_sticky_horizontal_scrollbar(driver: &WebDriver) -> Result<()> {
    let document = driver.find(By::Css("[data-gtl-diff-document]")).await?;
    let file = document.find(By::Css("[data-gtl-diff-file]")).await?;
    let rail = file
        .find(By::Css("[aria-label='Scroll diff horizontally']"))
        .await?;
    wait::until(
        "sticky horizontal scrollbar",
        wait::ASSERTION_TIMEOUT,
        || async {
            let viewport = document.rect().await?;
            let file = file.rect().await?;
            let bar = rail.rect().await?;
            Ok((rail.is_displayed().await?
                && file.y + file.height > viewport.y + viewport.height + 100.0
                && (bar.y + bar.height - viewport.y - viewport.height).abs() < 2.0)
                .then_some(()))
        },
    )
    .await?;
    let source = file
        .find(By::XPath(
            ".//*[@data-gtl-copy-text][starts-with(., 'wrapping-source ')]",
        ))
        .await?;
    let before = source.rect().await?;
    let bar = rail.rect().await?;
    driver
        .action_chain()
        .move_to(
            format!("{:.0}", bar.x + 20.0).parse()?,
            format!("{:.0}", bar.y + bar.height / 2.0).parse()?,
        )
        .click_and_hold()
        .move_by_offset(80, 0)
        .release()
        .perform()
        .await?;
    let movement = wait::until(
        "horizontal source scrolling",
        wait::ASSERTION_TIMEOUT,
        || async {
            let after = source.rect().await?;
            Ok((after.x < before.x - 20.0 && (after.y - before.y).abs() < 1.0).then_some(()))
        },
    )
    .await;
    let after = source.rect().await?;
    movement
        .map_err(|error| error.context(format!("source before={before:?}, after={after:?}")))?;
    support::evidence::capture(driver, "sticky-horizontal-scrollbar", true).await?;
    Ok(())
}

async fn verify_layouts(driver: &WebDriver, fixture: &OneShotFixture) -> Result<()> {
    for (layout, wrap_lines) in [
        ("unified", true),
        ("split", false),
        ("split", true),
        ("unified", false),
    ] {
        set_wrapping(driver, layout, wrap_lines).await?;
        fixture.forward()?;
        support::wait_for_active_diff(driver, "wrapping-alpha", "alpha-one-shot-marker").await?;
        assert_wrapping(driver, wrap_lines).await?;
        support::evidence::capture(
            driver,
            &format!("line-wrapping-{layout}-{wrap_lines}"),
            true,
        )
        .await?;
        resize_viewer(driver, fixture, 390, 800).await?;
        assert_wrapping(driver, wrap_lines).await?;
        support::evidence::capture(
            driver,
            &format!("line-wrapping-{layout}-{wrap_lines}-narrow"),
            true,
        )
        .await?;
        resize_viewer(driver, fixture, 1280, 900).await?;
    }
    Ok(())
}

async fn resize_viewer(
    driver: &WebDriver,
    fixture: &OneShotFixture,
    width: u32,
    height: u32,
) -> Result<()> {
    driver.set_window_rect(0, 0, width, height).await?;
    driver.refresh().await?;
    fixture.forward()?;
    support::wait_for_active_diff(driver, "wrapping-alpha", "alpha-one-shot-marker").await
}

async fn set_wrapping(driver: &WebDriver, layout: &str, wrap_lines: bool) -> Result<()> {
    support::selectors::by_test_id(driver, test_ids::VIEWER_MENU_TRIGGER)
        .await?
        .click()
        .await?;
    driver
        .find(By::Css("button[aria-label='User settings']"))
        .await?
        .click()
        .await?;
    let wrapping =
        support::selectors::by_test_id(driver, test_ids::VIEWER_SETTINGS_WRAP_LINES).await?;
    SelectElement::new(&wrapping)
        .await?
        .select_by_value(if wrap_lines { "true" } else { "false" })
        .await?;
    let layout_select = driver.find(By::Id("settings-layout")).await?;
    SelectElement::new(&layout_select)
        .await?
        .select_by_value(layout)
        .await?;
    driver
        .find(By::Css("button[type='submit']"))
        .await?
        .click()
        .await?;
    wait::until(
        "saved wrapping setting",
        wait::ASSERTION_TIMEOUT,
        || async {
            for toast in driver.find_all(By::Css(test_ids::TOAST.selector())).await? {
                if toast.text().await?.contains("Settings saved") {
                    return Ok(Some(()));
                }
            }
            Ok(None)
        },
    )
    .await?;
    support::selectors::by_test_id(driver, test_ids::TOAST_DISMISS)
        .await?
        .click()
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct LineGeometry {
    height: f64,
    line_height: f64,
    scroll_width: f64,
    client_width: f64,
    wrapping: bool,
}

async fn assert_wrapping(driver: &WebDriver, expected: bool) -> Result<()> {
    let result = wait::until("source line wrapping geometry", wait::ASSERTION_TIMEOUT, || async {
        let geometry: Option<LineGeometry> = driver.execute(r"
            const source = [...document.querySelectorAll('[data-gtl-copy-text]')].find(node => node.textContent.startsWith('wrapping-source '));
            if (!source) return null;
            const code = source.closest('code');
            const rows = source.closest('.diff-rows');
            return {
                height: code.getBoundingClientRect().height,
                line_height: parseFloat(getComputedStyle(code).lineHeight),
                scroll_width: rows.scrollWidth,
                client_width: rows.clientWidth,
                wrapping: source.closest('[data-gtl-diff-document]').dataset.wrapLines === 'true'
            };
        ", vec![]).await?.convert()?;
        let Some(geometry) = geometry else { return Ok(None); };
        let valid = if expected {
            geometry.height > geometry.line_height * 2.0 && geometry.scroll_width <= geometry.client_width + 1.0
        } else {
            geometry.height <= geometry.line_height + 1.0 && geometry.scroll_width > geometry.client_width + 20.0
        };
        Ok((geometry.wrapping == expected && valid).then_some(()))
    }).await;
    if let Err(error) = result {
        let state = driver.execute(r"
            return {innerWidth, outerWidth, viewport: document.documentElement.clientWidth,
              media: matchMedia('(min-width:1025px)').matches,
              rows: [...document.querySelectorAll('.diff-rows')].map(rows => ({width: rows.clientWidth, scrollWidth: rows.scrollWidth})),
              line: [...document.querySelectorAll('[data-gtl-copy-text]')].filter(node => node.textContent.startsWith('wrapping-source ')).map(node => ({rect: node.closest('code').getBoundingClientRect().toJSON(), style: getComputedStyle(node.closest('code')).whiteSpace}))};
        ", vec![]).await?.convert::<serde_json::Value>()?;
        return Err(error.context(format!("wrapping={expected}, browser={state}")));
    }
    Ok(())
}
