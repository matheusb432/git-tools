use anyhow::Context as _;
use gtl_web_contracts::test_ids;
use playwright_rs::expect;

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_opens_and_navigates_an_offline_diff() -> anyhow::Result<()> {
    let spec = support::Spec::start("raw-artifact-lifecycle").await?;
    let outcome = async {
        let repository = support::repository_with_raw_changes().await?;
        let artifact_url = support::render_raw_diff(&repository).await?;
        support::goto(&spec.session.page, &artifact_url).await?;

        let page = &spec.session.page;
        expect(page.get_by_text("alpha-marker", false))
            .to_be_visible()
            .await
            .context("show the generated offline diff")?;
        support::click(
            &support::get_button(page, "Collapse all"),
            "collapse raw diff files",
        )
        .await?;
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
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}
