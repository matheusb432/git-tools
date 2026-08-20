use anyhow::{Context as _, ensure};
use gtl_web_contracts::test_ids;
use playwright_rs::expect;

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_opens_and_navigates_an_offline_diff() -> anyhow::Result<()> {
    let spec = support::Spec::start("raw-artifact-lifecycle").await?;
    let outcome = async {
        let repository = support::repository_with_raw_changes().await?;
        let artifact_url = support::render_raw_diff(&repository, "split", "full").await?;
        spec.session.navigate_to_artifact(&artifact_url).await?;

        let page = &spec.session.page;
        expect(page.get_by_text("alpha-marker", false))
            .to_be_visible()
            .await
            .context("show the generated offline diff")?;
        let diff_files = page.locator("[data-gtl-diff-file]");
        expect(diff_files.clone())
            .to_have_count(3)
            .await
            .context("render every raw diff file")?;
        let alpha = page.locator("[data-gtl-diff-file][data-path='src/alpha.rs']");
        let beta = page.locator("[data-gtl-diff-file][data-path='src/beta.rs']");
        let large = page.locator("[data-gtl-diff-file][data-path='large.txt']");
        let alpha_open = page.locator("[data-gtl-diff-file][data-path='src/alpha.rs'][open]");
        let beta_open = page.locator("[data-gtl-diff-file][data-path='src/beta.rs'][open]");
        let large_open = page.locator("[data-gtl-diff-file][data-path='large.txt'][open]");
        expect(alpha.clone())
            .to_have_count(1)
            .await
            .context("render the first raw diff file")?;
        expect(beta.clone())
            .to_have_count(1)
            .await
            .context("render the second raw diff file")?;
        expect(large.clone())
            .to_have_count(1)
            .await
            .context("render the giant-line raw diff file")?;
        expect(alpha_open.clone())
            .to_have_count(1)
            .await
            .context("expand the first raw diff file initially")?;
        expect(beta_open.clone())
            .to_have_count(1)
            .await
            .context("expand the second raw diff file initially")?;
        expect(alpha.locator("[aria-label='split full diff rows']"))
            .to_have_count(1)
            .await
            .context("retain the requested split full presentation")?;
        support::click(
            &page
                .locator(test_ids::CHANGED_FILES_PANEL.selector())
                .get_by_text("large.txt", true),
            "navigate to the giant-line raw artifact file",
        )
        .await?;
        expect(large_open)
            .to_have_count(1)
            .await
            .context("open the giant-line raw artifact file")?;
        let long_line_control = large.locator("[data-gtl-action='toggle-long-line']");
        expect(long_line_control.clone())
            .to_have_count(1)
            .await
            .context("bound the giant source line behind a static control")?;
        expect(long_line_control.clone())
            .to_have_attribute("aria-expanded", "false")
            .await
            .context("collapse the giant source line initially")?;
        support::click(&long_line_control, "expand the giant raw artifact line").await?;
        expect(long_line_control)
            .to_have_attribute("aria-expanded", "true")
            .await
            .context("expand the giant source line with local glue")?;
        support::click(
            &support::get_button(page, "Collapse all"),
            "collapse raw diff files",
        )
        .await?;
        expect(alpha_open.clone())
            .to_have_count(0)
            .await
            .context("collapse the first raw diff file")?;
        expect(beta_open.clone())
            .to_have_count(0)
            .await
            .context("collapse the second raw diff file")?;
        expect(support::get_button(page, "Expand all"))
            .to_be_visible()
            .await
            .context("show the expand-all action after collapsing files")?;
        support::click(
            &page
                .locator(test_ids::CHANGED_FILES_PANEL.selector())
                .get_by_text("beta.rs", true),
            "navigate to the second raw artifact file",
        )
        .await?;
        expect(page.get_by_text("beta-marker", false))
            .to_be_visible()
            .await
            .context("show the selected raw diff file")?;
        expect(alpha_open)
            .to_have_count(0)
            .await
            .context("keep the unselected raw diff file collapsed")?;
        expect(beta_open.clone())
            .to_have_count(1)
            .await
            .context("expand the selected raw diff file")?;

        support::set_mobile_viewport(page).await?;
        support::click(
            &support::get_button(page, "Changed files"),
            "open raw mobile changed files",
        )
        .await?;
        support::click(
            &page.locator("dialog[open] [data-gtl-action='navigate-file'][title='src/beta.rs']"),
            "navigate from raw mobile changed files",
        )
        .await?;
        expect(beta_open)
            .to_have_count(1)
            .await
            .context("open the mobile-selected raw file")?;
        ensure!(
            page.url() == artifact_url,
            "raw artifact interaction left the production file URL"
        );
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}
