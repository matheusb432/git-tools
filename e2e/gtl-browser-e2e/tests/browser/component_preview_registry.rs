use std::env;

use anyhow::{Context as _, ensure};
use playwright_rs::{
    expect,
    protocol::{AriaRole, GetByRoleOptions, GotoOptions, Page, SelectOption, WaitUntil},
};

use crate::{harness::browser::operation, support};

const COMPONENT_PREVIEW_URL_ENVIRONMENT_VARIABLE: &str = "GTL_COMPONENT_PREVIEW_URL";

#[tokio::test(flavor = "multi_thread")]
async fn csr_registry_owns_stateful_preview_components() -> anyhow::Result<()> {
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
        expect(spec.session.page.locator(".dx-preview"))
            .to_have_css("color-scheme", "dark")
            .await
            .context("use dark catalog chrome by default")?;
        expect(spec.session.page.get_by_role(
            AriaRole::Heading,
            Some(GetByRoleOptions::default().name("Button").exact(true)),
        ))
        .to_have_count(1)
        .await
        .context("resolve the Wasm-initialized button showcase registration")?;

        let showcase_url = format!("{base_url}/showcase/button/interactive");
        navigate(
            &spec.session.page,
            &showcase_url,
            "open stateful component showcase",
        )
        .await?;
        let canvas = spec.session.page.locator("[data-dx-preview-ready='true']");
        expect(canvas.clone())
            .to_have_attribute("data-showcase", "button")
            .await
            .context("resolve the registered button showcase")?;
        expect(canvas.clone())
            .to_have_attribute("data-preview", "interactive")
            .await
            .context("resolve the direct-hook preview")?;

        support::click(
            &support::get_button(&spec.session.page, "Increment example count"),
            "increment direct-hook preview state",
        )
        .await?;
        expect(spec.session.page.get_by_text("1 click", true))
            .to_be_visible()
            .await
            .context("retain state in the preview component scope")?;
        support::click(
            &support::get_button(&spec.session.page, "Reset state"),
            "remount the direct-hook preview",
        )
        .await?;
        expect(spec.session.page.get_by_text("0 clicks", true))
            .to_be_visible()
            .await
            .context("reset state by replacing the preview component")?;

        let picker = spec
            .session
            .page
            .locator("select[aria-label='Button preview']");
        picker
            .select_option(SelectOption::Value("states".to_owned()), None)
            .await
            .context("switch away from the direct-hook preview")?;
        expect(canvas.clone())
            .to_have_attribute("data-preview", "states")
            .await
            .context("render the selected stateless preview")?;
        picker
            .select_option(SelectOption::Value("interactive".to_owned()), None)
            .await
            .context("switch back to the direct-hook preview")?;
        expect(canvas)
            .to_have_attribute("data-preview", "interactive")
            .await
            .context("restore the direct-hook preview component")?;
        expect(spec.session.page.get_by_text("0 clicks", true))
            .to_be_visible()
            .await
            .context("start a fresh hook scope after preview switching")?;
        ensure!(
            spec.session
                .page
                .url()
                .ends_with("/showcase/button/interactive"),
            "preview selection did not update the stable showcase route"
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
