use anyhow::{Context as _, ensure};
use playwright_rs::expect;

use crate::support;

const PRESENTATIONS: [(&str, &str, &str); 4] = [
    (
        "unified",
        "compact",
        "details.file[data-path='src/alpha.rs'] .diff-unified.diff-compact",
    ),
    (
        "unified",
        "full",
        "details.file[data-path='src/alpha.rs'] .diff-unified.diff-full",
    ),
    (
        "split",
        "compact",
        "details.file[data-path='src/alpha.rs'] .diff-split.diff-compact",
    ),
    (
        "split",
        "full",
        "details.file[data-path='src/alpha.rs'] .diff-split.diff-full",
    ),
];

#[tokio::test(flavor = "multi_thread")]
async fn raw_artifact_lifecycle() -> anyhow::Result<()> {
    let spec = support::Spec::start("raw-artifact-lifecycle").await?;
    let outcome = async {
        let repository = support::repository_with_raw_changes().await?;
        let mut artifact_url = String::new();

        for (layout, density, selector) in PRESENTATIONS {
            artifact_url = support::render_raw_diff(&repository, layout, density).await?;
            support::goto(&spec.session.page, &artifact_url).await?;
            let presentation = spec.session.page.locator(selector);
            expect(presentation.clone())
                .to_have_count(1)
                .await
                .with_context(|| format!("count {layout} {density} artifact presentations"))?;
            expect(presentation.first())
                .to_be_visible()
                .await
                .with_context(|| format!("render {layout} {density} artifact presentation"))?;
            ensure!(
                support::count(
                    &spec.session.page.locator("details.file"),
                    "count raw artifact files",
                )
                .await?
                    == 3,
                "raw artifact did not render the three fixture files"
            );
        }

        let page = &spec.session.page;
        expect(page.locator(".layout[data-gtl-enhanced='true']"))
            .to_have_count(1)
            .await
            .context("wait for one enhanced raw layout")?;
        expect(page.locator("aside.tree[aria-label='Changed files tree']"))
            .to_have_count(1)
            .await
            .context("find server-rendered changed-file tree")?;
        expect(page.locator("aside.shelf[aria-label='Commits in range']"))
            .to_have_count(1)
            .await
            .context("find server-rendered commit shelf")?;
        ensure!(
            support::count(
                &page.locator(".cline[data-sha]"),
                "count informational commit cards",
            )
            .await?
                > 0,
            "raw artifact rendered no commit cards"
        );
        expect(
            page.locator(".commit-select, .cline[role='button'], .cline[tabindex], .cline.active"),
        )
        .to_have_count(0)
        .await
        .context("exclude raw commit selection affordances")?;
        ensure!(
            support::count(
                &page.locator(".cline .sha[title='copy hash']"),
                "count raw commit hash copy actions",
            )
            .await?
                > 0,
            "raw artifact omitted commit hash copy actions"
        );

        expect(page.locator("details.file[data-path='large.txt']:not([open])"))
            .to_have_count(1)
            .await
            .context("verify the giant file starts collapsed")?;
        expect(page.locator("details.file[data-path='src/alpha.rs'][open]"))
            .to_have_count(1)
            .await
            .context("verify a small file starts expanded")?;

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
        support::click(&page.locator("button.foldall"), "expand all raw files").await?;
        expect(page.locator("details.file[open]"))
            .to_have_count(3)
            .await
            .context("expand every raw artifact file")?;

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
                "[data-preview-files-popover]:popover-open [data-file-target='f-src-beta-rs']",
            ),
            "navigate from raw mobile changed files",
        )
        .await?;
        expect(page.locator("details.file[data-path='src/beta.rs'][open]"))
            .to_have_count(1)
            .await
            .context("open the mobile-selected raw file")?;

        support::click(
            &support::get_button(page, "Commits in range"),
            "open raw mobile commits",
        )
        .await?;
        expect(page.locator("[data-preview-commits-popover]:popover-open .cline[data-sha]"))
            .to_be_visible()
            .await
            .context("show informational commits on mobile")?;
        support::click(
            &support::get_button(page, "Close commits in range"),
            "close raw mobile commits",
        )
        .await?;

        support::click(
            &support::get_button(page, "View settings"),
            "open raw mobile view settings",
        )
        .await?;
        support::click(
            &support::get_button(page, "Collapse or expand all files"),
            "collapse files from raw mobile view settings",
        )
        .await?;
        support::expect_every_file_is_collapsed(page).await?;
        support::click(
            &support::get_button(page, "View settings"),
            "reopen raw mobile view settings",
        )
        .await?;
        support::click(
            &support::get_button(page, "Collapse or expand all files"),
            "expand files from raw mobile view settings",
        )
        .await?;
        expect(page.locator("details.file[open]"))
            .to_have_count(3)
            .await
            .context("expand every file from raw mobile controls")?;
        support::click(
            &support::get_button(page, "View settings"),
            "reopen raw mobile view settings for evidence",
        )
        .await?;

        ensure!(
            page.url() == artifact_url,
            "raw artifact navigation left the production file URL"
        );
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}
