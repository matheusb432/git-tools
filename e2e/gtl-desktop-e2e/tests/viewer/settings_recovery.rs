use anyhow::{Result, ensure};
use thirtyfour::{By, Key, prelude::ElementQueryable as _};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn invalid_settings_offer_backup_reset_and_manual_repair() -> Result<()> {
    support::run_test("settings-recovery", |session| {
        Box::pin(recover_settings(session))
    })
    .await
}

async fn recover_settings(session: &mut support::session::TestSession) -> Result<()> {
    let raw = "[[projects]]\nname = \"rust-snake\"\nexclude_from_push_all = true\n";
    session.write_user_config(raw)?;
    session.driver().refresh().await?;
    let reset = session
        .driver()
        .query(By::XPath(
            "//button[normalize-space()='Back up and reset settings']",
        ))
        .first()
        .await?;
    let text = session.driver().find(By::Css("body")).await?.text().await?;
    ensure!(text.contains("User settings are invalid"));
    ensure!(text.contains("unknown field `exclude_from_push_all`"));
    ensure!(
        text.contains(
            &session
                .data_root()
                .join("config.toml")
                .display()
                .to_string()
        )
    );
    support::evidence::capture(session.driver(), "settings-recovery-desktop", true).await?;
    session.driver().set_window_rect(0, 0, 390, 800).await?;
    support::evidence::capture(session.driver(), "settings-recovery-narrow", true).await?;
    session.driver().set_window_rect(0, 0, 1600, 900).await?;
    reset.send_keys(Key::Enter).await?;
    session
        .driver()
        .query(By::Id("projects-heading"))
        .first()
        .await?;
    let backups = std::fs::read_dir(session.data_root())?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with("-backup.toml")
        })
        .collect::<Vec<_>>();
    ensure!(backups.len() == 1);
    ensure!(std::fs::read_to_string(backups[0].path())? == raw);
    ensure!(std::fs::read_to_string(session.data_root().join("config.toml"))?.is_empty());
    support::evidence::capture(session.driver(), "settings-recovery-complete", true).await?;
    session.write_user_config("theme = {{{")?;
    session.driver().refresh().await?;
    session
        .driver()
        .query(By::XPath(
            "//button[normalize-space()='Back up and reset settings']",
        ))
        .first()
        .await?;
    session.write_user_config("theme = \"light\"\n")?;
    session
        .driver()
        .find(By::XPath("//button[normalize-space()='Retry']"))
        .await?
        .click()
        .await?;
    session
        .driver()
        .query(By::Id("projects-heading"))
        .first()
        .await?;
    ensure!(
        std::fs::read_to_string(session.data_root().join("config.toml"))? == "theme = \"light\"\n"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_page_recovers_when_the_file_becomes_invalid_after_startup() -> Result<()> {
    support::run_test("settings-page-recovery", |session| {
        Box::pin(recover_settings_page(session))
    })
    .await
}

async fn recover_settings_page(session: &mut support::session::TestSession) -> Result<()> {
    session
        .driver()
        .query(By::Id("projects-heading"))
        .first()
        .await?;
    session.write_user_config("theme = 7")?;
    support::selectors::by_test_id(
        session.driver(),
        gtl_web_contracts::test_ids::VIEWER_MENU_TRIGGER,
    )
    .await?
    .click()
    .await?;
    session
        .driver()
        .find(By::Css("button[aria-label='User settings']"))
        .await?
        .click()
        .await?;
    session
        .driver()
        .query(By::XPath(
            "//button[normalize-space()='Back up and reset settings']",
        ))
        .first()
        .await?
        .click()
        .await?;
    session
        .driver()
        .query(By::XPath("//button[normalize-space()='Save settings']"))
        .first()
        .await?;
    ensure!(std::fs::read_to_string(session.data_root().join("config.toml"))?.is_empty());
    Ok(())
}
