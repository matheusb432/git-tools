use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, WebDriver};

use crate::support::{self, fixture::ProjectsFixture, wait};

#[tokio::test(flavor = "multi_thread")]
async fn user_imports_repositories_and_restores_their_comparisons() -> Result<()> {
    support::run_test("projects", |session| {
        Box::pin(async move {
            let fixture_root = tempfile::Builder::new()
                .prefix(".gtl-projects-")
                .tempdir_in(std::env::var_os("HOME").context("fixture home")?)?;
            let fixture = ProjectsFixture::create(fixture_root.path())?;
            let driver = session.driver();
            import_projects(driver, &fixture.root).await?;
            support::evidence::capture(driver, "projects-dashboard", true).await?;

            support::click(driver, action("projects-alpha", "Open diff")).await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker")
                .await?;
            support::click(driver, By::Css("button[aria-label='Modified files']")).await?;
            support::wait_for_active_diff(driver, "projects-alpha", "untracked-project-marker")
                .await?;
            support::click(driver, By::Css("button[aria-label='Modified files']")).await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker")
                .await?;

            ensure!(
                !main_text(driver).await?.contains("local-project-marker"),
                "the snapshot included uncommitted work"
            );
            support::click(driver, By::Css("a[aria-label='Projects']")).await?;
            support::click(driver, action("projects-alpha", "Open diff")).await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker")
                .await?;
            let snapshot =
                support::visible(driver, By::Css("[role='tab'][aria-selected='true']")).await?;
            support::context_click_element(driver, &snapshot).await?;
            support::click(
                driver,
                By::XPath("//*[@role='menu']//*[@role='menuitem'][contains(., 'Pin tab')]"),
            )
            .await?;
            support::visible(driver, By::Css("button[aria-label^='Unpin ']")).await?;

            session.restart().await?;
            let driver = session.driver();
            support::visible(driver, By::Css("button[aria-label^='Unpin ']")).await?;
            support::click(
                driver,
                By::XPath("//*[@role='tab' and contains(., 'projects-alpha')]"),
            )
            .await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker")
                .await
        })
    })
    .await
}

async fn import_projects(driver: &WebDriver, root: &std::path::Path) -> Result<()> {
    support::click(driver, By::Id("project-import-trigger")).await?;
    support::visible(
        driver,
        By::Css("#project-import-dialog input[placeholder^='~/my-projects']"),
    )
    .await?
    .send_keys(root.to_string_lossy().as_ref())
    .await?;
    support::click(driver, dialog_button("Scan")).await?;
    wait::until(
        "discovered repositories",
        wait::ASSERTION_TIMEOUT,
        || async {
            let rows = driver
                .find_all(By::Css(
                    "#project-import-dialog [data-testid='project-import-row']",
                ))
                .await?;
            Ok((rows.len() == 2).then_some(()))
        },
    )
    .await?;
    support::click(driver, dialog_button("Select all")).await?;
    support::click(driver, dialog_button("Add 2 selected")).await?;
    wait::until("imported repositories", wait::ASSERTION_TIMEOUT, || async {
        let mut created = 0;
        for row in driver
            .find_all(By::Css(
                "#project-import-dialog [data-testid='project-import-row']",
            ))
            .await?
        {
            if row.text().await?.contains("Created") {
                created += 1;
            }
        }
        Ok((created == 2).then_some(()))
    })
    .await?;
    support::click(driver, By::Css("button[aria-label='Close Add projects']")).await?;
    for name in ["projects-alpha", "projects-beta"] {
        support::visible(driver, By::Css(format!("tr[aria-label='{name}']"))).await?;
    }
    Ok(())
}

fn dialog_button(label: &str) -> By {
    By::XPath(format!(
        "//dialog[@id='project-import-dialog']//button[normalize-space()='{label}']"
    ))
}

fn action(project: &str, label: &str) -> By {
    By::Css(format!(
        "tr[aria-label='{project}'][aria-busy='false'] :is(a, button)[aria-label='{label}']"
    ))
}

async fn main_text(driver: &WebDriver) -> Result<String> {
    Ok(support::visible(driver, By::Css("main"))
        .await?
        .text()
        .await?)
}

#[tokio::test(flavor = "multi_thread")]
async fn user_finds_branch_commits_and_filters_a_snapshot() -> Result<()> {
    support::run_test("projects-commit-search", |session| {
        Box::pin(async move {
            let fixture_root = tempfile::Builder::new().prefix(".gtl-projects-").tempdir_in(
                std::env::var_os("HOME").context("fixture home")?,
            )?;
            let fixture = ProjectsFixture::create(fixture_root.path())?;
            let driver = session.driver();
            import_projects(driver, &fixture.root).await?;
            support::click(driver, action("projects-alpha", "Find commits")).await?;
            let search = support::visible(driver, By::Id("project-commit-search-input")).await?;
            search.send_keys("cmt prj wrk").await?;
            support::visible(driver, By::Css("#project-commit-search-dialog button[aria-label^='Open commit'][title^='committed project work']:enabled")).await?;
            support::evidence::capture(driver, "project-commit-finder", true).await?;
            search.send_keys(thirtyfour::Key::Down).await?;
            driver.action_chain().send_keys(thirtyfour::Key::Enter).perform().await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker").await?;
            support::click(driver, By::Css("a[aria-label='Projects']")).await?;
            support::click(driver, action("projects-alpha", "Open diff")).await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker").await?;
            support::evidence::capture(driver, "snapshot-commit-search-collapsed", true).await?;
            support::click(driver, By::Css("aside[aria-label='Commits'] button[aria-label='Find commits']")).await?;
            let input = support::visible(driver, By::Css("aside[aria-label='Commits'] input[type='search']")).await?;
            support::evidence::capture(driver, "snapshot-commit-search-expanded", true).await?;
            driver.action_chain().send_keys("cmt prj wrk").perform().await?;
            support::visible(driver, By::Css("aside[aria-label='Commits'] button[aria-label^='Open commit'][title^='committed project work']:enabled")).await?;
            support::evidence::capture(driver, "snapshot-commit-search-matches", true).await?;
            input.send_keys(thirtyfour::Key::Escape).await?;
            input.send_keys(thirtyfour::Key::Escape).await?;
            support::click(driver, By::Css("aside[aria-label='Commits'] button[aria-label='Find commits']")).await?;
            let input = support::visible(driver, By::Css("aside[aria-label='Commits'] input[type='search']")).await?;
            input.send_keys("base").await?;
            support::visible(driver, By::XPath("//aside[@aria-label='Commits']//*[contains(text(), 'No matching commits')]")).await?;
            support::click(driver, By::Css("aside[aria-label='Commits'] button[aria-label='Search scope']")).await?;
            support::evidence::capture(driver, "snapshot-commit-search-scope", true).await?;
            support::click(driver, By::XPath("//aside[@aria-label='Commits']//*[@popover]//button[normalize-space()='Active branch']")).await?;
            support::visible(driver, By::Css("aside[aria-label='Commits'] button[aria-label^='Open commit'][title^='base']:enabled")).await?;
            support::evidence::capture(driver, "snapshot-commit-search-branch", true).await?;
            input.send_keys(thirtyfour::Key::Enter).await?;
            support::wait_for_active_diff(driver, "projects-alpha", "base").await
        })
    }).await
}
