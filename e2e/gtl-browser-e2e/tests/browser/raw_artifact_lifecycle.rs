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
            page.locator("#f-src-alpha-rs [data-layout='split'][data-density='full']");
        expect(presentation.clone())
            .to_have_count(1)
            .await
            .context("count representative split full presentation")?;
        expect(presentation.first())
            .to_be_visible()
            .await
            .context("render representative split full presentation")?;
        expect(page.locator("main[data-gtl-artifact-ready='true'] [data-view-state='complete']"))
            .to_have_count(1)
            .await
            .context("wait for the client-rendered raw artifact")?;
        let syntax_span_count = page
            .locator("[data-gtl-diff-document] [class*='--sy-']")
            .count()
            .await
            .context("count highlighted Rust syntax spans")?;
        ensure!(
            syntax_span_count > 0,
            "raw artifact rendered no highlighted Rust syntax spans"
        );
        expect(page.locator("script[type='application/octet-stream']"))
            .to_have_count(0)
            .await
            .context("release compressed nodes after their lazy loads complete")?;
        support::click(
            &page.locator("aside[aria-label='Changed files'] [data-file-target='f-src-beta-rs']"),
            "navigate to the second raw artifact file",
        )
        .await?;
        expect(page.locator("details#f-src-beta-rs[open]"))
            .to_have_count(1)
            .await
            .context("open the second raw artifact file")?;

        support::click(
            &support::get_button(page, "Collapse all"),
            "collapse all raw files",
        )
        .await?;
        support::expect_every_file_is_collapsed(page).await?;

        support::reload(page).await?;
        expect(page.locator("main[data-gtl-artifact-ready='true'] [data-view-state='complete']"))
            .to_have_count(1)
            .await
            .context("remount one client-rendered raw artifact after reload")?;
        support::click(
            &support::get_button(page, "Collapse all"),
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
            &page.locator("dialog[open] [data-file-target='f-src-beta-rs']"),
            "navigate from raw mobile changed files",
        )
        .await?;
        expect(page.locator("details#f-src-beta-rs[open]"))
            .to_have_count(1)
            .await
            .context("open the mobile-selected raw file")?;

        ensure!(
            page.url() == artifact_url,
            "raw artifact navigation left the production file URL"
        );
        support::assert_no_browser_errors(page)?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

#[tokio::test(flavor = "multi_thread")]
async fn raw_artifact_reports_compressed_asset_failures() -> anyhow::Result<()> {
    let spec = support::Spec::start("raw-artifact-corrupt-runtime").await?;
    let outcome = async {
        let repository = support::repository_with_raw_changes().await?;
        for (layout, density, failure, expected) in [
            (
                "split",
                "compact",
                support::ArtifactFailure::DecompressionUnavailable,
                "cannot decompress",
            ),
            (
                "unified",
                "compact",
                support::ArtifactFailure::InvalidBase64,
                "not valid base64",
            ),
            (
                "split",
                "full",
                support::ArtifactFailure::InvalidGzip,
                "gzip stream is corrupt",
            ),
            (
                "unified",
                "full",
                support::ArtifactFailure::MissingPage,
                "does not contain the requested compressed diff page",
            ),
        ] {
            let artifact_url = support::render_raw_diff(&repository, layout, density).await?;
            support::apply_artifact_failure(&artifact_url, failure)?;
            support::goto(&spec.session.page, &artifact_url).await?;
            let alert = spec.session.page.locator("[role='alert']").first();
            expect(alert.clone())
                .to_be_visible()
                .await
                .context("render compressed runtime initialization error")?;
            let text = alert
                .text_content()
                .await
                .context("read compressed runtime initialization error")?
                .unwrap_or_default();
            ensure!(text.contains(expected), "unexpected runtime error: {text}");
            support::assert_no_browser_errors(&spec.session.page)?;
        }
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}
