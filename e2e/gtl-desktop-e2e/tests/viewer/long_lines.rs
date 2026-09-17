use anyhow::{Result, ensure};
use thirtyfour::{By, WebDriver, prelude::ElementQueryable as _};

use crate::support::{self, fixture::OneShotFixture};

#[tokio::test(flavor = "multi_thread")]
async fn user_reads_truncated_lines_and_copies_complete_source() -> Result<()> {
    support::run_test("viewer-long-lines", |session| {
        Box::pin(async move {
            let fixture = OneShotFixture::create_with_long_lines(session.data_root())?;
            fixture.forward()?;
            support::wait_for_active_diff(
                session.driver(),
                "long-lines",
                "94218 characters omitted",
            )
            .await?;
            navigate_files(session.driver()).await?;
            let file = session
                .driver()
                .find(By::Css("[data-gtl-diff-file][data-path='a.css']"))
                .await?;
            let preview = file.find(By::Css("[data-gtl-copy-text]")).await?;
            let text = preview.prop("textContent").await?.unwrap_or_default();
            ensure!(text.chars().count() < 600, "giant source reached the DOM");
            let copied = support::copy_selected_diff_line(session.driver(), "a.css", "xxx").await?;
            ensure!(
                copied.ends_with(&"x".repeat(94_718)),
                "copy returned only the preview"
            );
            ensure!(
                !copied.contains("characters omitted"),
                "desktop copy included the omission label"
            );
            support::evidence::capture(session.driver(), "bounded-css-preview", true).await?;
            Ok(())
        })
    })
    .await
}

async fn navigate_files(driver: &WebDriver) -> Result<()> {
    for (path, marker) in [
        ("work.txt", "alpha-one-shot-marker"),
        ("a.css", "94218 characters omitted"),
        ("b.css", "89515 characters omitted"),
        ("work.txt", "alpha-one-shot-marker"),
        ("a.css", "94218 characters omitted"),
    ] {
        driver
            .query(By::Css(format!("button[data-file-target][title='{path}']")))
            .and_displayed()
            .first()
            .await?
            .click()
            .await?;
        support::wait_for_active_diff(driver, "long-lines", marker).await?;
    }
    Ok(())
}
