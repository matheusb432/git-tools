use anyhow::{Context as _, ensure};
use gtl_web_contracts::test_ids;
use playwright_rs::{expect, protocol::Page};

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
        assert_selection_copy_context(page).await?;
        assert_path_copy_popover(page).await?;
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

async fn assert_selection_copy_context(page: &Page) -> anyhow::Result<()> {
    page.evaluate::<(), ()>(
        r#"() => {
            const file = document.querySelector(
                "[data-gtl-diff-file][data-path='src/alpha.rs']",
            );
            const source = [...file.querySelectorAll("[data-gtl-copy-text]")]
                .find((element) => element.textContent.includes("alpha-marker"));
            const range = document.createRange();
            range.selectNodeContents(source);
            const selection = window.getSelection();
            selection.removeAllRanges();
            selection.addRange(range);

            const root = document.querySelector("[data-gtl-artifact-ready='true']");
            root.addEventListener("copy", (event) => {
                globalThis.__gtlCopyObservation = [
                    event.clipboardData?.getData("text/plain") ?? "",
                    event.defaultPrevented,
                ];
            }, { once: true });
        }"#,
        None,
    )
    .await
    .context("select one added source line")?;
    page.keyboard()
        .press("Control+c", None)
        .await
        .context("copy the selected source line")?;
    let (text, prevented): (String, bool) = page
        .evaluate("() => globalThis.__gtlCopyObservation", None::<&()>)
        .await
        .context("observe the native copy event")?;

    ensure!(prevented, "the raw artifact did not intercept native copy");
    ensure!(
        text == "// * src/alpha.rs, lines: 5\nfn alpha_5() { println!(\"alpha-marker\"); }",
        "the raw artifact copied an unexpected source payload: {text:?}"
    );
    expect(page.get_by_text("Copied with context - lines 5", true))
        .to_be_visible()
        .await
        .context("confirm contextual selection copy")?;
    Ok(())
}

async fn assert_path_copy_popover(page: &Page) -> anyhow::Result<()> {
    page.evaluate::<(), ()>(
        r#"() => {
            Object.defineProperty(navigator, "clipboard", {
                configurable: true,
                value: {
                    writeText: async (value) => {
                        globalThis.__gtlWrittenPath = value;
                    },
                },
            });
        }"#,
        None,
    )
    .await
    .context("observe path clipboard writes")?;

    let file = page.locator("[data-gtl-diff-file][data-path='src/alpha.rs']");
    let trigger = file.locator("button[aria-label='Copy file path']");
    let popover = file.locator("[popover][aria-label='Copy file path']");
    support::click(&trigger, "open the raw file path popover").await?;
    expect(popover.clone())
        .to_be_visible()
        .await
        .context("show the raw file path options")?;
    expect(popover.locator("button[aria-label='Copy relative path']"))
        .to_be_visible()
        .await
        .context("show the relative path option")?;
    expect(popover.locator("button[aria-label='Copy absolute path']"))
        .to_be_visible()
        .await
        .context("show the absolute path option")?;
    expect(page.locator("[data-gtl-copy='code']"))
        .to_have_count(0)
        .await
        .context("omit the removed whole-file code action")?;

    let trigger_box = trigger
        .bounding_box()
        .await
        .context("measure the path popover trigger")?
        .context("path popover trigger has no layout box")?;
    let popover_box = popover
        .bounding_box()
        .await
        .context("measure the path popover")?
        .context("path popover has no layout box")?;
    let vertical_gap = popover_box.y - (trigger_box.y + trigger_box.height);
    let end_alignment =
        (popover_box.x + popover_box.width - trigger_box.x - trigger_box.width).abs();
    ensure!(
        (-0.5..=8.0).contains(&vertical_gap),
        "path popover is not anchored below its trigger: gap {vertical_gap}"
    );
    ensure!(
        end_alignment <= 1.0,
        "path popover is not end-aligned with its trigger: delta {end_alignment}"
    );

    let relative_action = popover.locator("button[data-gtl-copy='path']");
    support::click(&relative_action, "copy the raw relative file path").await?;
    expect(relative_action.locator("[data-gtl-copy-feedback][data-state='success']"))
        .to_be_visible()
        .await
        .context("confirm the relative path copy")?;
    let copied_path: String = page
        .evaluate("() => globalThis.__gtlWrittenPath", None::<&()>)
        .await
        .context("observe the copied relative path")?;
    ensure!(
        copied_path == "src/alpha.rs",
        "the path menu copied an unexpected value: {copied_path:?}"
    );

    page.keyboard()
        .press("Escape", None)
        .await
        .context("close the raw file path popover")?;
    expect(popover)
        .to_be_hidden()
        .await
        .context("dismiss the raw file path popover")?;
    expect(page.locator("[data-gtl-diff-file][data-path='src/alpha.rs'][open]"))
        .to_have_count(1)
        .await
        .context("keep the file expanded after using its path menu")?;
    Ok(())
}
