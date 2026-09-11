use std::env;

use anyhow::{Context as _, ensure};
use playwright_rs::{
    expect,
    protocol::{AriaRole, GetByRoleOptions, GotoOptions, Page, SelectOption, WaitUntil},
};

use crate::{harness::browser::operation, support};

const COMPONENT_PREVIEW_URL_ENVIRONMENT_VARIABLE: &str = "GTL_COMPONENT_PREVIEW_URL";

#[tokio::test(flavor = "multi_thread")]
async fn csr_registry_owns_stateful_component_stories() -> anyhow::Result<()> {
    let spec = support::Spec::start_web("component-preview-registry").await?;
    let outcome = async {
        let base_url = env::var(COMPONENT_PREVIEW_URL_ENVIRONMENT_VARIABLE)
            .context("component preview URL is missing")?;
        navigate(
            &spec.session.page,
            &base_url,
            "open component preview catalog",
        )
        .await?;
        expect(spec.session.page.locator("meta[name='darkreader-lock']"))
            .to_have_count(1)
            .await
            .context("disable Dark Reader for the self-themed catalog")?;
        expect(spec.session.page.locator(".dx-story"))
            .to_have_css("color-scheme", "dark")
            .await
            .context("use dark catalog chrome by default")?;
        expect(spec.session.page.get_by_role(
            AriaRole::Heading,
            Some(GetByRoleOptions::default().name("Button").exact(true)),
        ))
        .to_have_count(1)
        .await
        .context("resolve the Wasm-initialized button story set")?;

        let story_url = format!("{base_url}/stories/button/interactive");
        navigate(
            &spec.session.page,
            &story_url,
            "open stateful component story",
        )
        .await?;
        let canvas = spec.session.page.locator("[data-dx-story-ready='true']");
        expect(canvas.clone())
            .to_have_attribute("data-story-set", "button")
            .await
            .context("resolve the registered button story set")?;
        expect(canvas.clone())
            .to_have_attribute("data-story", "interactive")
            .await
            .context("resolve the direct-hook story")?;

        support::click(
            &support::get_button(&spec.session.page, "Increment example count"),
            "increment direct-hook story state",
        )
        .await?;
        expect(spec.session.page.get_by_text("1 click", true))
            .to_be_visible()
            .await
            .context("retain state in the story component scope")?;
        support::click(
            &support::get_button(&spec.session.page, "Reset state"),
            "remount the direct-hook story",
        )
        .await?;
        expect(spec.session.page.get_by_text("0 clicks", true))
            .to_be_visible()
            .await
            .context("reset state by replacing the story component")?;

        let picker = spec
            .session
            .page
            .locator("select[aria-label='Button story']");
        picker
            .select_option(SelectOption::Value("states".to_owned()), None)
            .await
            .context("switch away from the direct-hook story")?;
        expect(canvas.clone())
            .to_have_attribute("data-story", "states")
            .await
            .context("render the selected stateless story")?;
        picker
            .select_option(SelectOption::Value("interactive".to_owned()), None)
            .await
            .context("switch back to the direct-hook story")?;
        expect(canvas)
            .to_have_attribute("data-story", "interactive")
            .await
            .context("restore the direct-hook story component")?;
        expect(spec.session.page.get_by_text("0 clicks", true))
            .to_be_visible()
            .await
            .context("start a fresh hook scope after story switching")?;
        ensure!(
            spec.session
                .page
                .url()
                .ends_with("/stories/button/interactive"),
            "story selection did not update the stable route"
        );

        assert_commits_panel_preview(&spec.session.page, &base_url).await?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

#[tokio::test(flavor = "multi_thread")]
async fn file_picker_opens_the_keyboard_or_pointer_selection() -> anyhow::Result<()> {
    let spec = support::Spec::start_web("path-filter-popup").await?;
    let outcome = async {
        let base_url = env::var(COMPONENT_PREVIEW_URL_ENVIRONMENT_VARIABLE)?;
        let page = &spec.session.page;
        navigate(
            page,
            &format!("{base_url}/render/viewer-settings-redesign/filter-paths"),
            "open file picker story",
        )
        .await?;
        let input = page.get_by_role(
            AriaRole::Combobox,
            Some(
                GetByRoleOptions::default()
                    .name("Filter files by path")
                    .exact(true),
            ),
        );
        expect(page.locator("input[placeholder='Filter files by path']:focus"))
            .to_be_visible()
            .await?;
        input.press("ArrowDown", None).await?;
        let button_path = "crates/gtl-web/src/shared/ui/button.rs";
        expect(page.get_by_role(
            AriaRole::Option,
            Some(GetByRoleOptions::default().name(button_path).exact(true)),
        ))
        .to_have_attribute("aria-selected", "true")
        .await?;
        input.press("Enter", None).await?;
        expect(input.clone()).to_be_hidden().await?;
        expect(page.locator("details[data-path='crates/gtl-web/src/shared/ui/button.rs'][open]"))
            .to_be_visible()
            .await?;
        page.locator("#workspace-heading")
            .press("Control+p", None)
            .await?;
        input.fill("missing-file", None).await?;
        expect(page.get_by_text("No files match", true))
            .to_be_visible()
            .await?;
        expect(
            page.locator("aside[aria-label='Changed files']")
                .get_by_text("button.rs", true),
        )
        .to_be_visible()
        .await?;
        input.fill("dioxus-web", None).await?;
        page.get_by_role(
            AriaRole::Option,
            Some(
                GetByRoleOptions::default()
                    .name("docs/agents/dioxus-web.md")
                    .exact(true),
            ),
        )
        .click(None)
        .await?;
        expect(input.clone()).to_be_hidden().await?;
        expect(page.locator("details[data-path='docs/agents/dioxus-web.md'][open]"))
            .to_be_visible()
            .await?;
        page.locator("#workspace-heading")
            .press("Control+p", None)
            .await?;
        input.press("Escape", None).await?;
        expect(input).to_be_hidden().await?;
        Ok(())
    }
    .await;
    spec.finish(outcome).await
}

async fn assert_commits_panel_preview(page: &Page, base_url: &str) -> anyhow::Result<()> {
    let commits_panel_url = format!("{base_url}/render/viewer-settings-redesign/commits-panel");
    navigate(
        page,
        &commits_panel_url,
        "open the selected commit presentation",
    )
    .await?;
    let commits_panel = page.locator("aside[aria-label='Commits']");
    expect(commits_panel.clone())
        .to_be_visible()
        .await
        .context("render the commits panel story")?;
    expect(commits_panel.locator("[data-gtl-hover-popover-target]"))
        .to_have_count(4)
        .await
        .context("render every commit in the quiet stack")?;
    expect(commits_panel.locator("[aria-pressed='true']"))
        .to_have_count(1)
        .await
        .context("show one selected commit through its surface tone")?;
    commits_panel
        .locator("[aria-pressed='true']")
        .click(None)
        .await
        .context("click the selected commit again")?;
    expect(commits_panel.locator("[aria-pressed='true']"))
        .to_have_count(0)
        .await
        .context("return to the full comparison without a range button")?;
    expect(commits_panel.locator("svg"))
        .to_have_count(0)
        .await
        .context("omit decorative timeline markers from the commit stack")
}

async fn navigate(page: &Page, url: &str, label: &str) -> anyhow::Result<()> {
    operation(label, async {
        page.goto(
            url,
            GotoOptions::new().wait_until(WaitUntil::DomContentLoaded),
        )
        .await
        .with_context(|| label.to_owned())
        .map(|_| ())
    })
    .await
}
