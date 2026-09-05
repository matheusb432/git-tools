use anyhow::{Context as _, ensure};
use gtl_web_contracts::test_ids;
use playwright_rs::{
    expect,
    protocol::{Locator, Page, Viewport},
};

use crate::support;

#[tokio::test(flavor = "multi_thread")]
async fn user_opens_and_navigates_an_offline_diff() -> anyhow::Result<()> {
    let spec = support::Spec::start("raw-artifact-lifecycle").await?;
    let outcome = async {
        let repository = support::repository_with_raw_changes().await?;
        let artifact_url = support::render_raw_diff(&repository, "split", "full").await?;
        spec.session.navigate_to_artifact(&artifact_url).await?;

        let page = &spec.session.page;
        let files = RawArtifactFiles::new(page);
        assert_initial_artifact(page, &files).await?;
        assert_path_filter_popup(page).await?;
        assert_large_line_interaction(page, &files).await?;
        assert_collapsed_file_navigation(page, &files).await?;
        assert_mobile_file_navigation(page, &files).await?;
        ensure!(
            page.url() == artifact_url,
            "raw artifact interaction left the production file URL"
        );
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

async fn assert_path_filter_popup(page: &Page) -> anyhow::Result<()> {
    support::get_button(page, "Filter files by path")
        .click(None)
        .await?;
    let input = page.get_by_placeholder("Filter files by path", true);
    expect(page.locator("input[placeholder='Filter files by path']:focus"))
        .to_be_visible()
        .await?;
    input.fill("alpha.rs", None).await?;
    let options = page.locator("[data-gtl-path-filter] [role='listbox']");
    expect(options.get_by_text("alpha.rs", true))
        .to_be_visible()
        .await?;
    expect(options.get_by_text("beta.rs", true))
        .to_be_hidden()
        .await?;
    expect(
        page.locator(test_ids::CHANGED_FILES_PANEL.selector())
            .get_by_text("beta.rs", true),
    )
    .to_be_visible()
    .await?;
    input.press("Enter", None).await?;
    expect(input.clone()).to_be_hidden().await?;
    expect(page.locator("details[data-path='src/alpha.rs'][open]"))
        .to_be_visible()
        .await?;
    page.keyboard().press("Control+p", None).await?;
    input.fill("beta.rs", None).await?;
    options.get_by_text("beta.rs", true).click(None).await?;
    expect(input.clone()).to_be_hidden().await?;
    expect(page.locator("details[data-path='src/beta.rs'][open]"))
        .to_be_visible()
        .await?;
    page.keyboard().press("Control+p", None).await?;
    input.press("Escape", None).await?;
    expect(input).to_be_hidden().await?;
    Ok(())
}

struct RawArtifactFiles {
    alpha: Locator,
    beta: Locator,
    large: Locator,
    alpha_open: Locator,
    beta_open: Locator,
    large_open: Locator,
}

impl RawArtifactFiles {
    fn new(page: &Page) -> Self {
        Self {
            alpha: page.locator("[data-gtl-diff-file][data-path='src/alpha.rs']"),
            beta: page.locator("[data-gtl-diff-file][data-path='src/beta.rs']"),
            large: page.locator("[data-gtl-diff-file][data-path='large.txt']"),
            alpha_open: page.locator("[data-gtl-diff-file][data-path='src/alpha.rs'][open]"),
            beta_open: page.locator("[data-gtl-diff-file][data-path='src/beta.rs'][open]"),
            large_open: page.locator("[data-gtl-diff-file][data-path='large.txt'][open]"),
        }
    }
}

async fn assert_initial_artifact(page: &Page, files: &RawArtifactFiles) -> anyhow::Result<()> {
    expect(page.get_by_text("alpha-marker", false))
        .to_be_visible()
        .await
        .context("show the generated offline diff")?;
    expect(page.locator("[data-gtl-diff-file]"))
        .to_have_count(3)
        .await
        .context("render every raw diff file")?;
    expect(files.alpha.clone())
        .to_have_count(1)
        .await
        .context("render the first raw diff file")?;
    expect(files.beta.clone())
        .to_have_count(1)
        .await
        .context("render the second raw diff file")?;
    expect(files.large.clone())
        .to_have_count(1)
        .await
        .context("render the giant-line raw diff file")?;
    expect(files.alpha_open.clone())
        .to_have_count(1)
        .await
        .context("expand the first raw diff file initially")?;
    expect(files.beta_open.clone())
        .to_have_count(1)
        .await
        .context("expand the second raw diff file initially")?;
    expect(files.alpha.locator("[aria-label='split full diff rows']"))
        .to_have_count(1)
        .await
        .context("retain the requested split full presentation")?;
    assert_commit_details_hover_popover(page).await?;
    assert_selection_copy_context(page).await?;
    assert_path_copy_popover(page).await
}

async fn assert_commit_details_hover_popover(page: &Page) -> anyhow::Result<()> {
    let commits = page.locator(test_ids::COMMITS_PANEL.selector());
    let card = commits.locator("[data-gtl-hover-popover-target]").first();
    let popover = card.locator("[popover][role='tooltip']");
    expect(commits.locator("button"))
        .to_have_count(1)
        .await
        .context("keep the commit shelf free of a details trigger")?;
    expect(popover.clone())
        .to_be_hidden()
        .await
        .context("hide commit details before hover")?;

    card.hover(None)
        .await
        .context("hover the raw commit card")?;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    ensure!(
        popover.is_hidden().await?,
        "raw commit details opened before the 350-millisecond hover delay"
    );
    expect(popover.clone())
        .to_be_visible()
        .await
        .context("show raw commit details after sustained hover")?;
    expect(popover.clone())
        .to_contain_text("artifact change")
        .await
        .context("show the commit message in raw details")?;
    expect(popover.clone())
        .to_contain_text("Commit ID")
        .await
        .context("show the commit ID in raw details")?;
    expect(popover.clone())
        .to_contain_text("Date")
        .await
        .context("label the raw commit date concisely")?;
    expect(popover.clone())
        .not()
        .to_contain_text("Committed")
        .await
        .context("omit the redundant raw commit date label")?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let card_box = card
        .bounding_box()
        .await
        .context("measure the raw commit card")?
        .context("raw commit card has no layout box")?;
    let popover_box = popover
        .bounding_box()
        .await
        .context("measure the raw commit details")?
        .context("raw commit details have no layout box")?;
    ensure!(
        (popover_box.y - card_box.y).abs() <= 1.0,
        "raw commit details are detached from the card top"
    );
    ensure!(
        popover_box.x + popover_box.width <= card_box.x - 4.0,
        "raw commit details do not float beside the card: card={card_box:?}, popover={popover_box:?}"
    );

    page.locator("[data-gtl-diff-document]")
        .hover(None)
        .await
        .context("leave the raw commit card")?;
    expect(popover)
        .to_be_hidden()
        .await
        .context("hide raw commit details after hover leaves")
}

async fn assert_large_line_interaction(
    page: &Page,
    files: &RawArtifactFiles,
) -> anyhow::Result<()> {
    support::click(
        &page
            .locator(test_ids::CHANGED_FILES_PANEL.selector())
            .get_by_text("large.txt", true),
        "navigate to the giant-line raw artifact file",
    )
    .await?;
    expect(files.large_open.clone())
        .to_have_count(1)
        .await
        .context("open the giant-line raw artifact file")?;
    let long_line_control = files.large.locator("[data-gtl-action='toggle-long-line']");
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
        .context("expand the giant source line with local glue")
}

async fn assert_collapsed_file_navigation(
    page: &Page,
    files: &RawArtifactFiles,
) -> anyhow::Result<()> {
    page.set_viewport_size(Viewport {
        width: 1_200,
        height: 220,
    })
    .await
    .context("make the raw diff document vertically scrollable")?;
    let diff_document = page.locator("[data-gtl-diff-document]");
    let scrolled_top: f64 = diff_document
        .evaluate(
            "(element) => { element.scrollTop = element.scrollHeight; return element.scrollTop; }",
            None::<&()>,
        )
        .await
        .context("scroll near the end of the raw diff document")?;
    ensure!(
        scrolled_top > 0.0,
        "the raw diff fixture did not produce a vertical scroll range"
    );

    support::click(
        &support::get_button(page, "Collapse all"),
        "collapse raw diff files",
    )
    .await?;
    expect(files.alpha_open.clone())
        .to_have_count(0)
        .await
        .context("collapse the first raw diff file")?;
    expect(files.beta_open.clone())
        .to_have_count(0)
        .await
        .context("collapse the second raw diff file")?;
    expect(support::get_button(page, "Expand all"))
        .to_be_visible()
        .await
        .context("show the expand-all action after collapsing files")?;
    let collapsed_top: f64 = diff_document
        .evaluate("(element) => element.scrollTop", None::<&()>)
        .await
        .context("read the collapsed raw diff scroll position")?;
    ensure!(
        collapsed_top == 0.0,
        "collapsing raw diff files retained scroll position {collapsed_top}"
    );
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
    expect(files.alpha_open.clone())
        .to_have_count(0)
        .await
        .context("keep the unselected raw diff file collapsed")?;
    expect(files.beta_open.clone())
        .to_have_count(1)
        .await
        .context("expand the selected raw diff file")
}

async fn assert_mobile_file_navigation(
    page: &Page,
    files: &RawArtifactFiles,
) -> anyhow::Result<()> {
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
    expect(files.beta_open.clone())
        .to_have_count(1)
        .await
        .context("open the mobile-selected raw file")
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
    copy_raw_path_and_expect_popover_closed(
        &relative_action,
        &popover,
        "copy the raw relative file path",
    )
    .await?;
    assert_raw_written_path(page, "src/alpha.rs").await?;

    support::click(&trigger, "reopen the raw file path popover").await?;
    expect(popover.clone())
        .to_be_visible()
        .await
        .context("show the raw file path options again")?;
    let absolute_path = file
        .get_attribute("data-gtl-absolute-path")
        .await
        .context("read the raw absolute file path")?
        .context("raw diff file has no absolute path")?;
    let absolute_action = popover.locator("button[data-gtl-copy='absolute']");
    copy_raw_path_and_expect_popover_closed(
        &absolute_action,
        &popover,
        "copy the raw absolute file path",
    )
    .await?;
    assert_raw_written_path(page, &absolute_path).await?;

    expect(page.locator("[data-gtl-diff-file][data-path='src/alpha.rs'][open]"))
        .to_have_count(1)
        .await
        .context("keep the file expanded after using its path menu")?;
    Ok(())
}

async fn copy_raw_path_and_expect_popover_closed(
    action: &Locator,
    popover: &Locator,
    description: &str,
) -> anyhow::Result<()> {
    support::click(action, description).await?;
    expect(popover.clone())
        .to_be_hidden()
        .await
        .with_context(|| format!("close the raw path popover after {description}"))?;
    expect(action.locator("[data-gtl-copy-feedback][data-state='success']"))
        .to_have_count(1)
        .await
        .with_context(|| format!("confirm {description}"))
}

async fn assert_raw_written_path(page: &Page, expected: &str) -> anyhow::Result<()> {
    let copied_path: String = page
        .evaluate("() => globalThis.__gtlWrittenPath", None::<&()>)
        .await
        .context("observe the copied raw file path")?;
    ensure!(
        copied_path == expected,
        "the path menu copied an unexpected value: {copied_path:?}"
    );
    Ok(())
}
