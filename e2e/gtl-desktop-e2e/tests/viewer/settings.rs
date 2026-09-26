use anyhow::Result;
use thirtyfour::{By, WebDriver, components::SelectElement};

use crate::support::{self, fixture::OneShotFixture};

#[tokio::test(flavor = "multi_thread")]
async fn saved_viewer_settings_apply_and_survive_restart() -> Result<()> {
    support::run_test("settings", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create_named(session.data_root(), "settings-review")?;
            fixture.forward()?;
            let driver = session.driver();
            support::wait_for_active_diff(driver, "settings-review", "alpha-one-shot-marker")
                .await?;

            open_settings(driver).await?;
            select_value(driver, "settings-theme", "carbon").await?;
            select_value(driver, "settings-layout", "split").await?;
            select_value(driver, "settings-density", "full").await?;
            select_value(driver, "settings-language", "pt-BR").await?;
            support::click(driver, By::Css("button[type='submit']")).await?;
            support::visible(driver, By::Css("html[data-theme='carbon'][lang='pt-BR']")).await?;

            session.restart().await?;
            let driver = session.driver();
            support::visible(driver, By::Css("html[data-theme='carbon'][lang='pt-BR']")).await?;
            fixture.forward()?;
            support::wait_for_active_diff(driver, "settings-review", "alpha-one-shot-marker")
                .await?;
            support::visible(
                driver,
                By::Css("[data-gtl-diff-document][data-layout='split'][data-density='full']"),
            )
            .await?;
            Ok(())
        })
    })
    .await
}

pub(super) async fn open_settings(driver: &WebDriver) -> Result<()> {
    support::click(driver, By::Css("button[aria-label='Settings']")).await?;
    support::visible(driver, By::Css("#settings-theme")).await?;
    Ok(())
}

pub(super) async fn select_value(driver: &WebDriver, id: &str, value: &str) -> Result<()> {
    let select = driver.find(By::Id(id)).await?;
    SelectElement::new(&select)
        .await?
        .select_by_value(value)
        .await?;
    Ok(())
}
