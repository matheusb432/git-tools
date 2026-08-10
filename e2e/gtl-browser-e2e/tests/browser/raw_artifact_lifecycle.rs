use anyhow::{Context as _, ensure};
use playwright_rs::expect;

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn raw_artifact_lifecycle() -> anyhow::Result<()> {
    let spec = support::Spec::start("raw-artifact-lifecycle").await?;
    let outcome = async {
        let repository = support::repository_with_raw_changes().await?;
        let artifact_url = support::render_raw_diff(&repository, "split", "full").await?;
        support::goto(&spec.session.page, &artifact_url).await?;

        let page = &spec.session.page;
        let presentation =
            page.locator("details.file[data-path='src/alpha.rs'] .diff-split.diff-full");
        expect(presentation.clone())
            .to_have_count(1)
            .await
            .context("count representative split full presentation")?;
        expect(presentation.first())
            .to_be_visible()
            .await
            .context("render representative split full presentation")?;
        expect(page.locator(".layout[data-gtl-enhanced='true']"))
            .to_have_count(1)
            .await
            .context("wait for one enhanced raw layout")?;
        support::click(
            &page.locator(".tree-body .tfile[data-path='src/beta.rs'] .tlabel"),
            "navigate to the second raw artifact file",
        )
        .await?;
        expect(page.locator(".tree-body .tfile.cur[data-path='src/beta.rs']"))
            .to_have_count(1)
            .await
            .context("mark the second raw artifact file current")?;
        expect(page.locator("details.file[data-path='src/beta.rs'][open]"))
            .to_have_count(1)
            .await
            .context("open the second raw artifact file")?;

        support::click(&page.locator("button.foldall"), "collapse all raw files").await?;
        support::expect_every_file_is_collapsed(page).await?;

        support::reload(page).await?;
        expect(page.locator(".layout[data-gtl-enhanced='true']"))
            .to_have_count(1)
            .await
            .context("remount one enhancer lifecycle after raw reload")?;
        support::click(
            &page.locator("button.foldall"),
            "collapse files after reload",
        )
        .await?;
        support::expect_every_file_is_collapsed(page).await?;

        support::set_mobile_viewport(page).await?;
        support::click(
            &support::get_button(page, "Changed files"),
            "open raw mobile changed files",
        )
        .await?;
        support::click(
            &page.locator(
                "[data-artifact-files-popover]:popover-open [data-file-target='f-src-beta-rs']",
            ),
            "navigate from raw mobile changed files",
        )
        .await?;
        expect(page.locator("details.file[data-path='src/beta.rs'][open]"))
            .to_have_count(1)
            .await
            .context("open the mobile-selected raw file")?;

        ensure!(
            page.url() == artifact_url,
            "raw artifact navigation left the production file URL"
        );
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}
