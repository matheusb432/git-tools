use anyhow::{Context as _, Result, ensure};
use thirtyfour::{By, WebDriver};

use crate::support::{
    self,
    fixture::{OneShotFixture, ProjectsFixture},
    wait,
};

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
            pause_and_resume_project(driver, "projects-beta").await?;
            discard_unsaved_edit(driver, "projects-alpha", session.data_root()).await?;

            support::click(driver, project_link("projects-alpha")).await?;
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
            support::click(driver, project_link("projects-alpha")).await?;
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

async fn pause_and_resume_project(driver: &WebDriver, name: &str) -> Result<()> {
    let row = By::Css(format!("tr[aria-label='{name}']"));
    support::click(driver, action(name, &format!("Pause {name}"))).await?;
    support::dismiss_toast(driver, &format!("Paused {name}.")).await?;
    wait_until_absent(driver, row.clone()).await?;
    super::settings::select_value(driver, "projects-status-filter", "paused").await?;
    support::click(driver, action(name, &format!("Resume {name}"))).await?;
    support::dismiss_toast(driver, &format!("Resumed {name}.")).await?;
    wait_until_absent(driver, row.clone()).await?;
    super::settings::select_value(driver, "projects-status-filter", "active").await?;
    support::visible(driver, row).await?;
    Ok(())
}

async fn discard_unsaved_edit(
    driver: &WebDriver,
    name: &str,
    data_root: &std::path::Path,
) -> Result<()> {
    let discard_prompt = By::Css("#project-edit-discard-dialog[open]");
    support::click(driver, action(name, &format!("Edit {name}"))).await?;
    support::visible(driver, By::Id("project-edit-branch"))
        .await?
        .send_keys("-draft")
        .await?;
    let snapshot = OneShotFixture::create_named(data_root, "edit-navigation")?;
    snapshot.forward()?;
    support::wait_for_active_diff(driver, "edit-navigation", "alpha-one-shot-marker").await?;
    support::click(driver, By::Css("a[aria-label='Projects']")).await?;
    let branch = support::visible(driver, By::Id("project-edit-branch"))
        .await?
        .value()
        .await?;
    ensure!(
        branch.is_some_and(|branch| branch.ends_with("-draft")),
        "returning to Projects lost the draft"
    );
    support::click(driver, dialog_action("project-edit-dialog", "Cancel")).await?;
    support::visible(driver, discard_prompt.clone()).await?;
    support::click(
        driver,
        dialog_action("project-edit-discard-dialog", "Cancel"),
    )
    .await?;
    wait_until_absent(driver, discard_prompt.clone()).await?;
    let branch = support::visible(driver, By::Id("project-edit-branch"))
        .await?
        .value()
        .await?;
    ensure!(
        branch.is_some_and(|branch| branch.ends_with("-draft")),
        "keeping the edit lost the draft"
    );
    support::click(
        driver,
        By::Css(format!("button[aria-label='Close Edit {name}']")),
    )
    .await?;
    support::click(
        driver,
        dialog_action("project-edit-discard-dialog", "Discard changes"),
    )
    .await?;
    wait_until_absent(driver, By::Id("project-edit-dialog")).await
}

async fn wait_until_absent(driver: &WebDriver, locator: By) -> Result<()> {
    wait::until("element removal", wait::ASSERTION_TIMEOUT, || async {
        Ok(driver
            .find_all(locator.clone())
            .await?
            .is_empty()
            .then_some(()))
    })
    .await
}

fn dialog_button(label: &str) -> By {
    dialog_action("project-import-dialog", label)
}

fn dialog_action(dialog: &str, label: &str) -> By {
    By::XPath(format!(
        "//dialog[@id='{dialog}']//button[normalize-space()='{label}']"
    ))
}

fn project_link(project: &str) -> By {
    By::Css(format!(
        "tr[aria-label='{project}'][aria-busy='false'] a[data-testid='project-table-name']"
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
            set_commit_search_time(driver, By::Id("project-commit-search-input-from"), "2099-01-01T00:00:00").await?;
            search.click().await?;
            support::visible(driver, By::XPath("//*[@id='project-commit-search-dialog']//*[contains(text(), 'No matching commits')]")).await?;
            support::click(driver, By::Css("#project-commit-search-dialog button[aria-label='Clear time filter']")).await?;
            search.send_keys("cmt prj wrk").await?;
            support::visible(driver, By::Css("#project-commit-search-dialog button[aria-label^='Open commit'][title^='committed project work']:enabled")).await?;
            support::evidence::capture(driver, "project-commit-finder", true).await?;
            search.send_keys(thirtyfour::Key::Down).await?;
            driver.action_chain().send_keys(thirtyfour::Key::Enter).perform().await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker").await?;
            support::click(driver, By::Css("a[aria-label='Projects']")).await?;
            support::click(driver, project_link("projects-alpha")).await?;
            support::wait_for_active_diff(driver, "projects-alpha", "committed-project-marker").await?;
            support::evidence::capture(driver, "snapshot-commit-search-collapsed", true).await?;
            support::click(driver, By::Css("aside[aria-label='Commits'] button[aria-label='Find commits']")).await?;
            let input = support::visible(driver, By::Css("aside[aria-label='Commits'] input[type='search']")).await?;
            support::evidence::capture(driver, "snapshot-commit-search-expanded", true).await?;
            driver.action_chain().send_keys("cmt prj wrk").perform().await?;
            support::visible(driver, By::Css("aside[aria-label='Commits'] button[aria-label^='Open commit'][title^='committed project work']:enabled")).await?;
            set_commit_search_time(driver, By::Css("aside[aria-label='Commits'] input[id$='-until']"), "2000-01-01T00:00:00").await?;
            input.click().await?;
            support::visible(driver, By::XPath("//aside[@aria-label='Commits']//*[contains(text(), 'No matching commits')]")).await?;
            support::click(driver, By::Css("aside[aria-label='Commits'] button[aria-label='Clear time filter']")).await?;
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

async fn set_commit_search_time(driver: &WebDriver, locator: By, value: &str) -> Result<()> {
    let input = support::visible(driver, locator).await?;
    // WebKit types datetime-local segments rather than accepting an ISO value through send_keys.
    driver.execute(
        "const input = arguments[0]; input.value = arguments[1]; input.dispatchEvent(new Event('change', { bubbles: true }));",
        vec![input.to_json()?, serde_json::json!(value)],
    ).await?;
    Ok(())
}
