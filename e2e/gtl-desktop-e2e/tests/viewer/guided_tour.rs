use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, Key, WebDriver};

use crate::support::{
    self,
    fixture::{OneShotFixture, ProjectsFixture},
    wait,
};

#[tokio::test(flavor = "multi_thread")]
async fn screen_guides_replay_and_preserve_the_viewer_session() -> Result<()> {
    support::run_test("guided-tour", |session| {
        Box::pin(async move {
            let root = tempfile::Builder::new()
                .prefix(".gtl-guides-")
                .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
            let fixture = ProjectsFixture::create(root.path())?;
            super::projects::import_projects(session.driver(), &fixture.root).await?;
            visit_project_guides(session.driver()).await?;
            session.restart().await?;
            let driver = session.driver();
            ensure!(
                driver
                    .find_all(By::Css("[data-tour-launch='projects'] .guided-tour-unseen"))
                    .await?
                    .is_empty(),
                "a seen guide became unseen after restart"
            );

            support::click(
                driver,
                By::Css("[data-project-row$='/projects-alpha'][aria-busy='false'] a[data-testid='project-table-name']"),
            )
            .await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker")
                .await?;
            OneShotFixture::create_named(session.data_root(), "guide-other")?.forward()?;
            support::wait_for_active_diff(driver, "guide-other", "alpha-one-shot-marker").await?;
            let route = driver.current_url().await?;
            launch(driver, "diff-workspace").await?;
            check_resized_guide(driver).await?;
            driver
                .action_chain()
                .key_down(Key::Control)
                .send_keys(Key::Tab)
                .key_up(Key::Control)
                .perform()
                .await?;
            ensure!(
                driver.current_url().await? == route,
                "a viewer tab shortcut escaped the tour"
            );
            driver
                .action_chain()
                .key_down(Key::Control)
                .send_keys("f")
                .key_up(Key::Control)
                .perform()
                .await?;
            driver
                .action_chain()
                .send_keys(Key::Escape)
                .perform()
                .await?;
            expect_closed_and_focused(driver, "diff-workspace").await?;
            visit_workspace_guides(driver).await?;

            support::click(driver, By::Id("viewer-settings-button")).await?;
            visit_settings_guides(driver).await?;
            support::click(driver, By::Css("[data-settings-section='locale']")).await?;
            support::click(
                driver,
                By::Css("input[name='settings-language'][value='pt-BR']"),
            )
            .await?;
            support::visible(driver, By::Css("html[lang='pt-BR']")).await?;
            visit_settings_guides(driver).await?;
            launch(driver, "settings-keybindings").await?;
            support::evidence::capture(driver, "guided-tour-portuguese", true).await?;
            driver
                .action_chain()
                .send_keys(Key::Escape)
                .perform()
                .await?;
            expect_closed_and_focused(driver, "settings-keybindings").await?;
            support::click(driver, By::Css("[data-tour='settings-back']")).await?;
            visit_workspace_guides(driver).await?;
            support::click(driver, By::Css("[data-tour='diff-workspace-projects']")).await?;
            visit_project_guides(driver).await
        })
    })
    .await
}

async fn check_resized_guide(driver: &WebDriver) -> Result<()> {
    support::evidence::capture(driver, "guided-tour-workspace", true).await?;
    let original = driver.get_window_rect().await?;
    driver.set_window_rect(20, 20, 520, 360).await?;
    wait::until(
        "tour fits resized window",
        wait::ASSERTION_TIMEOUT,
        || async {
            let viewport = driver
                .find(By::Css("dialog[data-guided-tour]"))
                .await?
                .rect()
                .await?;
            let card = driver
                .find(By::Css(".guided-tour-card"))
                .await?
                .rect()
                .await?;
            Ok((card.x >= viewport.x
                && card.y >= viewport.y
                && card.x + card.width <= viewport.x + viewport.width
                && card.y + card.height <= viewport.y + viewport.height)
                .then_some(()))
        },
    )
    .await?;
    support::visible(driver, By::Css(".guided-tour-actions button:last-child")).await?;
    support::evidence::capture(driver, "guided-tour-small-window", true).await?;
    driver
        .set_window_rect(
            original.x,
            original.y,
            original.width.try_into()?,
            original.height.try_into()?,
        )
        .await?;
    Ok(())
}

async fn visit_project_guides(driver: &WebDriver) -> Result<()> {
    complete(driver, "projects").await?;
    for (trigger, parent, tour) in [
        (
            By::Id("project-import-trigger"),
            "project-import-dialog",
            "project-import",
        ),
        (
            By::Css("[data-project-row$='/projects-alpha'] button[id^='project-edit-']"),
            "project-edit-dialog",
            "project-comparison",
        ),
        (
            By::Css("[data-project-row$='/projects-alpha'] button[id^='project-commit-search-']"),
            "project-commit-search-dialog",
            "commit-finder",
        ),
        (
            By::Id("all-snapshots"),
            "project-snapshots-dialog",
            "snapshot-history",
        ),
    ] {
        support::click(driver, trigger).await?;
        launch(driver, tour).await?;
        driver
            .action_chain()
            .send_keys(Key::Escape)
            .perform()
            .await?;
        expect_closed_and_focused(driver, tour).await?;
        support::visible(driver, By::Css(format!("#{parent}[open]"))).await?;
        complete(driver, tour).await?;
        driver
            .action_chain()
            .send_keys(Key::Escape)
            .perform()
            .await?;
        wait::until(
            "parent guide dialog closes",
            wait::ASSERTION_TIMEOUT,
            || async {
                Ok(driver
                    .find_all(By::Css(format!("#{parent}[open]")))
                    .await?
                    .is_empty()
                    .then_some(()))
            },
        )
        .await?;
    }
    Ok(())
}

async fn visit_workspace_guides(driver: &WebDriver) -> Result<()> {
    for tour in [
        "diff-workspace",
        "diff-files",
        "diff-commits",
        "diff-reading",
    ] {
        complete(driver, tour).await?;
    }
    Ok(())
}

async fn visit_settings_guides(driver: &WebDriver) -> Result<()> {
    complete(driver, "settings").await?;
    for section in ["appearance", "locale", "snapshots", "git", "keybindings"] {
        support::click(
            driver,
            By::Css(format!("[data-settings-section='{section}']")),
        )
        .await?;
        complete(driver, &format!("settings-{section}")).await?;
    }
    Ok(())
}

async fn launch(driver: &WebDriver, tour: &str) -> Result<()> {
    support::click(driver, By::Css(format!("[data-tour-launch='{tour}']"))).await?;
    support::visible(driver, By::Css("dialog[data-guided-tour][open]")).await?;
    wait::until("tour initial focus", wait::ASSERTION_TIMEOUT, || async {
        Ok((driver
            .active_element()
            .await?
            .attr("data-dialog-content-initial-focus")
            .await?
            .as_deref()
            == Some("true"))
        .then_some(()))
    })
    .await?;
    Ok(())
}

async fn complete(driver: &WebDriver, tour: &str) -> Result<()> {
    launch(driver, tour).await?;
    for _ in 0..16 {
        let next = support::visible(
            driver,
            By::Css("dialog[data-guided-tour] .guided-tour-actions button:last-child"),
        )
        .await?;
        let finish = matches!(next.text().await?.as_str(), "Finish" | "Concluir");
        next.click().await?;
        if finish {
            return expect_closed_and_focused(driver, tour).await;
        }
    }
    anyhow::bail!("guide {tour} did not finish within 16 steps")
}

async fn expect_closed_and_focused(driver: &WebDriver, tour: &str) -> Result<()> {
    wait::until(
        "guide closes and restores focus",
        wait::ASSERTION_TIMEOUT,
        || async {
            Ok((driver
                .find_all(By::Css("dialog[data-guided-tour]"))
                .await?
                .is_empty()
                && driver
                    .active_element()
                    .await?
                    .attr("data-tour-launch")
                    .await?
                    .as_deref()
                    == Some(tour))
            .then_some(()))
        },
    )
    .await?;
    Ok(())
}
