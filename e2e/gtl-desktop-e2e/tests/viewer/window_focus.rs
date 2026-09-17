use anyhow::{Result, ensure};
use gtl_web_contracts::test_ids;
use thirtyfour::{By, WebDriver, components::SelectElement};

use crate::support::{self, fixture::OneShotFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_saves_window_focus_and_opens_diffs_without_restart() -> Result<()> {
    support::run_test("viewer-window-focus", |session| {
        Box::pin(run_window_focus_journey(session))
    })
    .await
}

async fn run_window_focus_journey(session: &mut support::session::TestSession) -> Result<()> {
    let fixture = OneShotFixture::create_named(session.data_root(), "window-focus")?;
    let configuration = session.data_root().join("config.toml");
    for enabled in [false, true] {
        open_settings(session.driver()).await?;
        let select = session
            .driver()
            .find(By::Id("settings-focus-window-on-diff"))
            .await?;
        ensure!(
            select.value().await?.as_deref() == Some(if enabled { "false" } else { "true" }),
            "saved window focus value was not loaded"
        );
        let before = std::fs::read_to_string(&configuration).ok();
        SelectElement::new(&select)
            .await?
            .select_by_value(if enabled { "true" } else { "false" })
            .await?;
        ensure!(
            std::fs::read_to_string(&configuration).ok() == before,
            "window focus saved before Submit"
        );
        session
            .driver()
            .find(By::Css("button[type='submit']"))
            .await?
            .click()
            .await?;
        wait::until("window focus saved", wait::ASSERTION_TIMEOUT, || async {
            Ok(std::fs::read_to_string(&configuration)
                .ok()
                .filter(|text| text.contains(&format!("focus_window_on_diff = {enabled}")))
                .map(|_| ()))
        })
        .await?;
        support::selectors::by_test_id(session.driver(), test_ids::TOAST_DISMISS)
            .await?
            .click()
            .await?;
        support::evidence::capture(session.driver(), "window-focus-settings-desktop", true).await?;
        session.driver().set_window_rect(0, 0, 390, 800).await?;
        session
            .driver()
            .find(By::Id("settings-focus-window-on-diff"))
            .await?
            .scroll_into_view()
            .await?;
        support::evidence::capture(session.driver(), "window-focus-settings-narrow", true).await?;
        session.driver().set_window_rect(0, 0, 1280, 900).await?;
        fixture.forward()?;
        support::wait_for_active_diff(session.driver(), "window-focus", "alpha-one-shot-marker")
            .await?;
    }
    session.restart().await?;
    open_settings(session.driver()).await?;
    ensure!(
        session
            .driver()
            .find(By::Id("settings-focus-window-on-diff"))
            .await?
            .value()
            .await?
            .as_deref()
            == Some("true"),
        "window focus did not survive restart"
    );
    Ok(())
}

async fn open_settings(driver: &WebDriver) -> Result<()> {
    support::selectors::by_test_id(driver, test_ids::VIEWER_MENU_TRIGGER)
        .await?
        .click()
        .await?;
    driver
        .find(By::Css("button[aria-label='User settings']"))
        .await?
        .click()
        .await?;
    wait::until("settings loaded", wait::ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(By::Id("settings-focus-window-on-diff"))
            .await?
            .into_iter()
            .next())
    })
    .await?;
    Ok(())
}
