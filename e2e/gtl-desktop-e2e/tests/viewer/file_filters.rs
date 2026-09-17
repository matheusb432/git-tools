use anyhow::{Context as _, Result, ensure};
use gtl_infra::user_config::TomlSettingsStore;
use gtl_web_contracts::test_ids;
use thirtyfour::{By, Key, WebDriver, WebElement, prelude::ElementQueryable as _};

use crate::support::{self, catalogue::ProjectCatalogue, fixture::OneShotFixture, wait};

const INITIAL_CONFIG: &str = "theme = \"mirage\"\n[diff]\nexclude = ['lock']\n";

#[tokio::test(flavor = "multi_thread")]
async fn excluded_extensions_auto_save_and_restore_global_defaults() -> Result<()> {
    support::run_test("file-extensions", |session| Box::pin(journey(session))).await
}

async fn choice(driver: &WebDriver, extension: &str, selected: bool) -> Result<WebElement> {
    let result = wait::until("extension selection", wait::ASSERTION_TIMEOUT, || async {
        let choices = driver.find_all(By::Css(format!("#diff-extension-filters [role='option'][aria-label='Exclude .{extension}'][aria-selected='{selected}']"))).await?;
        for choice in choices {
            if choice.is_displayed().await? && choice.is_enabled().await? { return Ok(Some(choice)); }
        }
        Ok(None)
    }).await;
    match result {
        Ok(choice) => Ok(choice),
        Err(error) => {
            let html = driver
                .find(By::Id("diff-extension-filters"))
                .await?
                .prop("outerHTML")
                .await?;
            Err(error).with_context(|| {
                format!("expected .{extension} selected={selected}; dropdown: {html:?}")
            })
        }
    }
}

async fn file_result(driver: &WebDriver, count: usize, path: &str) -> Result<()> {
    driver
        .query(By::Css(format!(
            "[data-total-files='{count}'] [data-path='{path}']"
        )))
        .and_displayed()
        .first()
        .await?;
    Ok(())
}

async fn open_filters(driver: &WebDriver) -> Result<()> {
    support::selectors::by_test_id(driver, test_ids::DIFF_EXTENSION_FILTERS_TRIGGER)
        .await?
        .click()
        .await?;
    Ok(())
}

async fn click_button(driver: &WebDriver, label: &str) -> Result<()> {
    driver
        .query(By::XPath(format!(
            "//*[@id='diff-extension-filters']//button[normalize-space()='{label}']"
        )))
        .and_enabled()
        .first()
        .await?
        .click()
        .await?;
    Ok(())
}

async fn saved(session: &support::session::TestSession, expected: Option<&[&str]>) -> Result<()> {
    let store = TomlSettingsStore::new(Some(session.data_root().join("config.toml")));
    wait::until(
        "saved project exclusions",
        wait::ASSERTION_TIMEOUT,
        || async {
            let (settings, _) = store.load_viewer_settings()?;
            let current = settings
                .diff_exclusions()
                .for_project(&"filter-review".try_into()?);
            let extensions = current.map(|value| {
                value
                    .extensions()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            });
            Ok((extensions.as_deref() == expected).then_some(()))
        },
    )
    .await
}

async fn journey(session: &mut support::session::TestSession) -> Result<()> {
    session.write_user_config(INITIAL_CONFIG)?;
    let fixture = OneShotFixture::create_with_excluded_files(session.data_root(), "filter-review")?;
    fixture.forward()?;
    support::wait_for_active_diff(session.driver(), "filter-review", "alpha-one-shot-marker")
        .await?;
    open_filters(session.driver()).await?;
    ensure!(
        session
            .driver()
            .find_all(By::Css("#diff-extension-filters input"))
            .await?
            .is_empty(),
        "the default dropdown must only show excluded chips"
    );
    support::evidence::capture(session.driver(), "extensions-compact-dropdown", true).await?;
    session
        .driver()
        .find(By::Css("button[aria-label='Reveal .lock']"))
        .await?
        .click()
        .await?;
    click_button(session.driver(), "Exclude extension…").await?;
    choice(session.driver(), "lock", false).await?;
    file_result(session.driver(), 2, "Cargo.lock").await?;
    choice(session.driver(), "txt", false)
        .await?
        .click()
        .await?;
    choice(session.driver(), "txt", true).await?;
    file_result(session.driver(), 1, "Cargo.lock").await?;
    click_button(session.driver(), "Restore global defaults").await?;
    choice(session.driver(), "lock", true).await?;
    choice(session.driver(), "txt", false).await?;
    file_result(session.driver(), 1, "work.txt").await?;
    ensure!(std::fs::read_to_string(session.data_root().join("config.toml"))? == INITIAL_CONFIG);
    ProjectCatalogue::open(session.data_root())?.set_projects(&[(
        "FLT",
        "Filter review",
        fixture.repository(),
    )])?;
    session.driver().refresh().await?;
    support::wait_for_active_diff(session.driver(), "filter-review", "alpha-one-shot-marker")
        .await?;
    project_edits(session, &fixture).await?;
    Ok(())
}

async fn project_edits(
    session: &mut support::session::TestSession,
    fixture: &OneShotFixture,
) -> Result<()> {
    open_filters(session.driver()).await?;
    click_button(session.driver(), "Exclude extension…").await?;
    choice(session.driver(), "lock", true)
        .await?
        .click()
        .await?;
    choice(session.driver(), "lock", false).await?;
    saved(session, Some(&[])).await?;
    let input = session
        .driver()
        .find(By::Id("extension-filter-search"))
        .await?;
    input.send_keys(".snap").await?;
    click_button(session.driver(), "Add .snap").await?;
    choice(session.driver(), "snap", true).await?;
    saved(session, Some(&["snap"])).await?;
    choice(session.driver(), "lock", false)
        .await?
        .click()
        .await?;
    choice(session.driver(), "txt", false)
        .await?
        .click()
        .await?;
    choice(session.driver(), "lock", true).await?;
    choice(session.driver(), "txt", true).await?;
    saved(session, Some(&["lock", "snap", "txt"])).await?;
    ensure!(session.driver().find_all(By::Css("#diff-extension-filters input[type='checkbox'], #diff-extension-filters [aria-busy='true'], #diff-extension-filters button:disabled")).await?.is_empty());
    support::evidence::capture(session.driver(), "extensions-search-multiselect", true).await?;
    click_button(session.driver(), "Restore global defaults").await?;
    saved(session, None).await?;
    choice(session.driver(), "lock", true).await?;
    choice(session.driver(), "txt", false).await?;
    let input = session
        .driver()
        .find(By::Id("extension-filter-search"))
        .await?;
    input.send_keys(".json").await?;
    input.send_keys(Key::Enter).await?;
    input.send_keys(Key::Escape).await?;
    session
        .driver()
        .find(By::Id("diff-extension-filters-trigger"))
        .await?
        .send_keys(Key::Escape)
        .await?;
    support::selectors::by_test_id(session.driver(), test_ids::VIEWER_MENU_TRIGGER)
        .await?
        .click()
        .await?;
    session
        .driver()
        .find(By::Css("button[aria-label='User settings']"))
        .await?
        .click()
        .await?;
    saved(session, Some(&["json", "lock"])).await?;
    edit_defaults(session).await?;
    fixture.forward()?;
    support::wait_for_active_diff(session.driver(), "filter-review", "alpha-one-shot-marker")
        .await?;
    open_filters(session.driver()).await?;
    click_button(session.driver(), "Exclude extension…").await?;
    choice(session.driver(), "lock", true).await?;
    choice(session.driver(), "txt", false).await?;
    click_button(session.driver(), "Restore global defaults").await?;
    saved(session, None).await?;
    choice(session.driver(), "lock", false).await?;
    choice(session.driver(), "txt", true).await?;
    session
        .driver()
        .find(By::Id("extension-filter-search"))
        .await?
        .send_keys(Key::Escape)
        .await?;
    support::evidence::capture(
        session.driver(),
        "extensions-restored-global-defaults",
        true,
    )
    .await?;
    Ok(())
}

async fn edit_defaults(session: &mut support::session::TestSession) -> Result<()> {
    let defaults = session
        .driver()
        .query(By::Css("section[aria-label='Default diff exclusions']"))
        .first()
        .await?;
    defaults
        .find(By::Css("button[aria-label='Reveal .lock']"))
        .await?
        .click()
        .await?;
    defaults
        .find(By::XPath(
            ".//button[normalize-space()='Exclude extension…']",
        ))
        .await?
        .click()
        .await?;
    let input = defaults
        .find(By::Id("settings-default-exclusion-search"))
        .await?;
    input.send_keys(".txt").await?;
    defaults
        .find(By::XPath(".//button[normalize-space()='Add .txt']"))
        .await?
        .click()
        .await?;
    defaults
        .find(By::XPath(".//button[normalize-space()='Save']"))
        .await?
        .click()
        .await?;
    wait::until(
        "global exclusions saved",
        wait::ASSERTION_TIMEOUT,
        || async {
            let raw = std::fs::read_to_string(session.data_root().join("config.toml"))?;
            Ok(raw.contains("\"txt\"").then_some(()))
        },
    )
    .await?;
    support::evidence::capture(session.driver(), "extensions-settings", true).await?;
    Ok(())
}
