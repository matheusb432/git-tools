use anyhow::Context as _;
use playwright_rs::{
    expect,
    protocol::{AriaRole, GetByRoleOptions},
};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_opens_and_navigates_an_offline_diff() -> anyhow::Result<()> {
    // The artifact browser context disables page JavaScript before navigation.
    let spec = support::Spec::start("raw-artifact-lifecycle").await?;
    let outcome = async {
        let repository = support::repository_with_raw_changes().await?;
        let artifact_url = support::render_raw_diff(&repository, "split", "full").await?;
        let page = &spec.session.page;
        support::set_mobile_viewport(page).await?;
        spec.session.navigate_to_artifact(&artifact_url).await?;

        let file_index = page.locator("nav.file-index");
        expect(file_index.clone()).not().to_be_visible().await?;
        let commits = page.locator("details.commits");
        expect(commits.locator("ol")).not().to_be_visible().await?;
        let commits_summary = commits.locator(":scope > summary");
        commits_summary.focus().await?;
        commits_summary.press("Enter", None).await?;
        expect(commits.locator("ol")).to_be_visible().await?;
        commits_summary.press("Enter", None).await?;
        expect(commits.locator("ol")).not().to_be_visible().await?;

        let alpha = page
            .locator("details.file")
            .filter(playwright_rs::protocol::FilterOptions::default().has_text("src/alpha.rs"));
        expect(alpha.clone())
            .to_contain_text("alpha-marker")
            .await?;
        let summary = alpha.locator(":scope > summary");
        summary.click(None).await?;
        expect(alpha.locator("table")).not().to_be_visible().await?;
        summary.focus().await?;
        summary.press("Enter", None).await?;
        expect(alpha.locator("table")).to_be_visible().await?;

        page.locator(".files-sidebar > details > summary")
            .click(None)
            .await?;
        expect(file_index).to_be_visible().await?;
        page.get_by_role(
            AriaRole::Link,
            Some(GetByRoleOptions::default().name("large.txt").exact(true)),
        )
        .click(None)
        .await?;
        let long_line = page.locator("details.long-line");
        let full_line = long_line.locator(".line-full");
        expect(full_line.clone()).not().to_be_visible().await?;
        long_line.locator("summary").click(None).await?;
        expect(full_line.clone()).to_be_visible().await?;
        let expected = format!("large-marker-{}", "x".repeat(250_000));
        let text = full_line
            .text_content()
            .await?
            .context("expanded line has no text")?;
        anyhow::ensure!(text == expected, "expansion lost source content");
        long_line.locator("summary").click(None).await?;
        expect(full_line).not().to_be_visible().await?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}
