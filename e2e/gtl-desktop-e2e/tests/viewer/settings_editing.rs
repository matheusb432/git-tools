use anyhow::{Result, ensure};
use gtl_infra::user_config::TomlSettingsStore;
use gtl_web_contracts::test_ids;
use thirtyfour::{
    By, Key, WebDriver, WebElement, components::SelectElement, prelude::ElementQueryable as _,
};

use crate::support::{self, fixture::OneShotFixture, wait};

const INITIAL_CONFIG: &str = r#"theme = "mirage"
layout = "unified"
density = "compact"

[push]
confirm = true

[diff]
exclude = ["lock"]

[[projects]]
name = "settings-review"
diff = { exclude = ["snap"] }
"#;

#[tokio::test(flavor = "multi_thread")]
async fn settings_edits_validate_persist_refresh_and_recover_from_conflicts() -> Result<()> {
    support::run_test("settings-editing", |session| {
        Box::pin(settings_journey(session))
    })
    .await
}

async fn settings_journey(session: &mut support::session::TestSession) -> Result<()> {
    session.write_user_config(INITIAL_CONFIG)?;
    let fixture = OneShotFixture::create_named(session.data_root(), "settings-review")?;
    fixture.forward()?;
    support::wait_for_active_diff(session.driver(), "settings-review", "alpha-one-shot-marker")
        .await?;
    open_settings(session.driver()).await?;

    let configuration_path = session.data_root().join("config.toml");
    let before = std::fs::read_to_string(&configuration_path)?;
    select_value(session.driver(), "settings-theme", "light").await?;
    select_value(session.driver(), "settings-layout", "split").await?;
    select_value(session.driver(), "settings-density", "full").await?;
    select_value(session.driver(), "settings-push-confirmation", "false").await?;
    ensure!(
        std::fs::read_to_string(&configuration_path)? == before,
        "scalar settings were persisted before Submit"
    );
    save_settings(session.driver()).await?;
    wait_for_scalar_settings(session).await?;
    dismiss_toast(session.driver()).await?;

    fixture.forward()?;
    support::wait_for_active_diff(session.driver(), "settings-review", "alpha-one-shot-marker")
        .await?;
    session
        .driver()
        .query(By::Css(
            "[data-gtl-diff-document][data-layout='split'][data-density='full']",
        ))
        .and_displayed()
        .first()
        .await?;

    open_settings(session.driver()).await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css("section[aria-label='Project diff exclusions']"))
            .await?
            .is_empty(),
        "project exclusions must remain in the project-scoped editor"
    );
    validate_and_save_default_exclusion(session).await?;

    recover_from_concurrent_edit(session, &configuration_path).await
}

async fn recover_from_concurrent_edit(
    session: &support::session::TestSession,
    configuration_path: &std::path::Path,
) -> Result<()> {
    let configuration_path = configuration_path.to_path_buf();

    select_value(session.driver(), "settings-push-confirmation", "true").await?;
    let current = std::fs::read_to_string(&configuration_path)?;
    let concurrent = current.replacen("theme = \"light\"", "theme = \"noir\"", 1);
    ensure!(
        current != concurrent,
        "could not prepare the concurrent settings edit"
    );
    session.write_user_config(&concurrent)?;
    save_settings(session.driver()).await?;

    let alert = wait::until(
        "inline settings conflict",
        wait::ASSERTION_TIMEOUT,
        || async {
            for alert in session
                .driver()
                .find_all(By::Css("p[role='alert']"))
                .await?
            {
                if alert
                    .text()
                    .await?
                    .contains("Settings changed since this page loaded")
                {
                    return Ok(Some(alert));
                }
            }
            Ok(None)
        },
    )
    .await?;
    ensure!(alert.is_displayed().await?);
    ensure!(
        session
            .driver()
            .find(By::Id("settings-push-confirmation"))
            .await?
            .value()
            .await?
            .as_deref()
            == Some("true"),
        "the scalar draft was lost after the conflict"
    );
    ensure!(std::fs::read_to_string(&configuration_path)? == concurrent);
    support::evidence::capture(session.driver(), "settings-edit-conflict", true).await?;

    session
        .driver()
        .find(By::XPath("//button[normalize-space()='Reload settings']"))
        .await?
        .click()
        .await?;
    wait::until(
        "reloaded effective theme",
        wait::ASSERTION_TIMEOUT,
        || async {
            let resolved = session
                .driver()
                .find(By::Css("section[aria-label='Resolved viewer settings']"))
                .await?;
            Ok(resolved.text().await?.contains("Noir").then_some(()))
        },
    )
    .await?;
    ensure!(
        session
            .driver()
            .find(By::Id("settings-theme"))
            .await?
            .value()
            .await?
            .as_deref()
            == Some("light"),
        "reloading the settings revision discarded the pending theme draft"
    );
    save_settings(session.driver()).await?;
    wait::until(
        "reapplied settings draft",
        wait::ASSERTION_TIMEOUT,
        || async {
            let store = TomlSettingsStore::new(Some(configuration_path.clone()));
            let (settings, _) = store.load_viewer_settings()?;
            Ok((settings
                .theme()
                .is_some_and(|theme| theme.to_string() == "light")
                && settings.push_confirmation_required())
            .then_some(()))
        },
    )
    .await?;
    support::evidence::capture(session.driver(), "settings-edit-complete", true).await?;
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
    wait::until("editable settings", wait::ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(By::Id("settings-push-confirmation"))
            .await?
            .into_iter()
            .next())
    })
    .await?;
    Ok(())
}

async fn select_value(driver: &WebDriver, id: &str, value: &str) -> Result<()> {
    let select = driver.find(By::Id(id)).await?;
    SelectElement::new(&select)
        .await?
        .select_by_value(value)
        .await?;
    Ok(())
}

async fn save_settings(driver: &WebDriver) -> Result<()> {
    driver
        .find(By::XPath("//button[normalize-space()='Save settings']"))
        .await?
        .click()
        .await?;
    Ok(())
}

async fn wait_for_scalar_settings(session: &support::session::TestSession) -> Result<()> {
    let configuration_path = session.data_root().join("config.toml");
    wait::until(
        "persisted scalar settings",
        wait::ASSERTION_TIMEOUT,
        || async {
            let store = TomlSettingsStore::new(Some(configuration_path.clone()));
            let (settings, _) = store.load_viewer_settings()?;
            let options = settings.viewer_render_options();
            Ok((settings
                .theme()
                .is_some_and(|theme| theme.to_string() == "light")
                && options.layout().to_string() == "split"
                && options.density().to_string() == "full"
                && !settings.push_confirmation_required())
            .then_some(()))
        },
    )
    .await
}

async fn validate_and_save_default_exclusion(
    session: &support::session::TestSession,
) -> Result<()> {
    let section = session
        .driver()
        .find(By::Css("section[aria-label='Default diff exclusions']"))
        .await?;
    section
        .find(By::XPath(
            ".//button[normalize-space()='Exclude extension…']",
        ))
        .await?
        .click()
        .await?;
    let input = section
        .find(By::Id("settings-default-exclusion-search"))
        .await?;
    input.send_keys("Cargo.lock").await?;
    wait::until(
        "invalid extension feedback",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok((input.attr("aria-invalid").await?.as_deref() == Some("true")).then_some(()))
        },
    )
    .await?;
    let error_id = input
        .attr("aria-describedby")
        .await?
        .ok_or_else(|| anyhow::anyhow!("invalid extension was not connected to its error"))?;
    ensure!(
        section
            .find(By::Id(&error_id))
            .await?
            .text()
            .await?
            .contains("final file extension")
    );
    input.send_keys(Key::Control + "a").await?;
    input.send_keys(Key::Backspace).await?;
    input.send_keys(".txt").await?;
    section
        .find(By::XPath(".//button[normalize-space()='Add .txt']"))
        .await?
        .click()
        .await?;
    save_exclusions(&section).await?;
    wait_for_exclusions(session, &["lock", "txt"]).await?;
    dismiss_toast(session.driver()).await
}

async fn save_exclusions(editor: &WebElement) -> Result<()> {
    editor
        .find(By::XPath(".//button[normalize-space()='Save']"))
        .await?
        .click()
        .await?;
    Ok(())
}

async fn wait_for_exclusions(
    session: &support::session::TestSession,
    expected: &[&str],
) -> Result<()> {
    let configuration_path = session.data_root().join("config.toml");
    wait::until("persisted exclusions", wait::ASSERTION_TIMEOUT, || async {
        let store = TomlSettingsStore::new(Some(configuration_path.clone()));
        let (settings, _) = store.load_viewer_settings()?;
        let actual = settings
            .diff_exclusions()
            .default_exclusions()
            .extensions()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let project = settings
            .diff_exclusions()
            .for_project(&"settings-review".try_into()?)
            .ok_or_else(|| anyhow::anyhow!("project exclusions were removed"))?;
        Ok((actual == expected && project.extensions() == ["snap"]).then_some(()))
    })
    .await
}

async fn dismiss_toast(driver: &WebDriver) -> Result<()> {
    support::selectors::by_test_id(driver, test_ids::TOAST_DISMISS)
        .await?
        .click()
        .await?;
    Ok(())
}
