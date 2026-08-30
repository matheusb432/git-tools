use std::env;

use anyhow::{Context as _, ensure};
use playwright_rs::{
    expect,
    protocol::{AriaRole, GetByRoleOptions, GotoOptions, Page, SelectOption, WaitUntil},
};

use crate::{harness::browser::operation, support};

const COMPONENT_PREVIEW_URL_ENVIRONMENT_VARIABLE: &str = "GTL_COMPONENT_PREVIEW_URL";

#[tokio::test(flavor = "multi_thread")]
async fn csr_registry_owns_stateful_variant_components() -> anyhow::Result<()> {
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
        expect(spec.session.page.get_by_role(
            AriaRole::Heading,
            Some(GetByRoleOptions::default().name("Button").exact(true)),
        ))
        .to_have_count(1)
        .await
        .context("resolve the Wasm-initialized button story registration")?;

        let story_url = format!("{base_url}/story/button/interactive");
        navigate(
            &spec.session.page,
            &story_url,
            "open stateful component story",
        )
        .await?;
        let canvas = spec
            .session
            .page
            .locator("[data-dioxus-storybook-ready='true']");
        expect(canvas.clone())
            .to_have_attribute("data-story", "button")
            .await
            .context("resolve the registered button story")?;
        expect(canvas.clone())
            .to_have_attribute("data-variant", "interactive")
            .await
            .context("resolve the direct-hook variant")?;

        support::click(
            &support::get_button(&spec.session.page, "Increment example count"),
            "increment direct-hook variant state",
        )
        .await?;
        expect(spec.session.page.get_by_text("1 click", true))
            .to_be_visible()
            .await
            .context("retain state in the variant component scope")?;
        support::click(
            &support::get_button(&spec.session.page, "Reset state"),
            "remount the direct-hook variant",
        )
        .await?;
        expect(spec.session.page.get_by_text("0 clicks", true))
            .to_be_visible()
            .await
            .context("reset state by replacing the variant component")?;

        let picker = spec
            .session
            .page
            .locator("select[aria-label='Button variant']");
        picker
            .select_option(SelectOption::Value("states".to_owned()), None)
            .await
            .context("switch away from the direct-hook variant")?;
        expect(canvas.clone())
            .to_have_attribute("data-variant", "states")
            .await
            .context("render the selected stateless variant")?;
        picker
            .select_option(SelectOption::Value("interactive".to_owned()), None)
            .await
            .context("switch back to the direct-hook variant")?;
        expect(canvas)
            .to_have_attribute("data-variant", "interactive")
            .await
            .context("restore the direct-hook variant component")?;
        expect(spec.session.page.get_by_text("0 clicks", true))
            .to_be_visible()
            .await
            .context("start a fresh hook scope after variant switching")?;
        ensure!(
            spec.session
                .page
                .url()
                .ends_with("/story/button/interactive"),
            "variant selection did not update the stable story route"
        );
        Ok(())
    }
    .await;
    spec.finish(outcome).await
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
